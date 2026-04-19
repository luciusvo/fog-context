//! Provider traits - abstract interfaces for LLM access.
//!
//! These traits live in fog-types (Ring 0) so that inner layers (fog-harness)
//! can depend on the abstraction without importing the implementation (fog-llm).
//!
//! Two levels of abstraction:
//! - `LlmProvider`: Raw single-model access (complete, stream)
//! - `GatewayService`: Production gateway with rotation, budget, tracking

use async_trait::async_trait;

use crate::config::RequestContext;
use crate::llm::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Error type for LLM operations.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Provider not registered: {0}")]
    ProviderNotRegistered(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

// ---------------------------------------------------------------------------
// LlmProvider - single-model raw access
// ---------------------------------------------------------------------------

/// Universal trait - every LLM provider must implement this.
///
/// Consumers interact with LLMs exclusively through this interface,
/// enabling hot-swap between providers without code changes.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider identifier (e.g., "gemini", "anthropic", "openai", "mock").
    fn id(&self) -> &str;

    /// Static model metadata for token budget calculations.
    fn model_info(&self) -> ModelInfo;

    /// Send messages and get a complete response.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Send messages and stream response chunks via callback.
    async fn complete_stream(
        &self,
        request: CompletionRequest,
        on_chunk: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, LlmError>;

    /// Hot-swap the API key used by this provider.
    /// Returns Err if the provider doesn't support key rotation (e.g., local).
    fn swap_api_key(&mut self, new_key: &str) -> Result<(), LlmError> {
        let _ = new_key;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// GatewayService - production gateway abstraction
// ---------------------------------------------------------------------------

/// What the orchestration layer (fog-harness) needs from the LLM gateway.
///
/// This trait captures the MINIMAL interface the agentic loop requires:
/// - Send a completion request (gateway handles rotation, failover, budget internally)
/// - Know which model is currently active
/// - Switch model (for task router)
/// - Get usage report
///
/// PATTERN_DECISION: Level 2 (Composition via trait)
/// Justification: GatewayService is a genuine boundary between orchestration
/// (domain logic) and external API access (infrastructure). LlmProvider handles
/// single-model calls; GatewayService handles the full production flow.
#[async_trait]
pub trait GatewayService: Send + Sync {
    /// Complete a request with full gateway features (rotation, budget, tracking).
    ///
    /// The `ctx` parameter provides tracking dimensions (session_id, conversation_id, etc.)
    async fn gateway_complete(
        &mut self,
        request: CompletionRequest,
        ctx: &RequestContext,
    ) -> Result<CompletionResponse, LlmError>;

    /// Get the currently active model ref (e.g., "gemini/gemini-2.5-flash").
    fn active_model_ref(&self) -> &str;

    /// Switch to a different model.
    fn switch_model(&mut self, model_ref: &str) -> Result<(), LlmError>;

    /// Get aggregated usage report.
    fn usage_report(&self) -> GatewayUsageReport;
}

// ---------------------------------------------------------------------------
// GatewayUsageReport - the report type harness can use
// ---------------------------------------------------------------------------

/// Usage report from the gateway, visible to inner layers.
///
/// This is the "output" type of GatewayService - inner layers can read usage
/// data without knowing how the gateway tracks it internally.
#[derive(Debug, Clone, Default)]
pub struct GatewayUsageReport {
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub total_estimated_cost: f64,
    pub total_requests: usize,
    pub failover_count: usize,
    pub advisor_count: usize,
}
