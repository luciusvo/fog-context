//! LLM request/response types for the universal gateway.
//!
//! Provider-agnostic: every LLM (Gemini, Anthropic, OpenAI, local)
//! communicates through these shared types.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

/// Who sent the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Content of a message — text or tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    ToolResult {
        tool_use_id: String,
        tool_name: String,
        content: String,
        is_error: bool,
    },
}

// ---------------------------------------------------------------------------
// Completion Request / Response
// ---------------------------------------------------------------------------

/// Provider-agnostic completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
}

/// Provider-agnostic completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    /// Which model actually responded (for multi-model setups).
    pub model_id: String,
}

// ---------------------------------------------------------------------------
// Tool definitions & calls
// ---------------------------------------------------------------------------

/// A tool the LLM can invoke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters.
    pub input_schema: serde_json::Value,
}

/// A tool invocation from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Token usage
// ---------------------------------------------------------------------------

/// Token consumption for a single completion.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    /// Tokens read from cache (provider-specific, 0 if unsupported).
    pub cache_read_tokens: usize,
    /// Tokens written to cache.
    pub cache_write_tokens: usize,
}

impl TokenUsage {
    /// Total tokens consumed (input + output).
    pub fn total(&self) -> usize {
        self.input_tokens + self.output_tokens
    }
}

// ---------------------------------------------------------------------------
// Stop reason
// ---------------------------------------------------------------------------

/// Why the LLM stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Model finished naturally.
    EndTurn,
    /// Hit output token limit.
    MaxTokens,
    /// Model wants to call a tool.
    ToolUse,
    /// Hit a stop sequence.
    StopSequence,
}

// ---------------------------------------------------------------------------
// Model info
// ---------------------------------------------------------------------------

/// Static metadata about an LLM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "gemini-2.5-pro", "claude-sonnet-4-20250514").
    pub model_id: String,
    /// Provider name (e.g., "gemini", "anthropic", "openai").
    pub provider: String,
    pub max_input_tokens: usize,
    pub max_output_tokens: usize,
    pub supports_tools: bool,
    pub supports_vision: bool,
    /// Cost per 1000 input tokens in USD.
    pub cost_per_1k_input: f64,
    /// Cost per 1000 output tokens in USD.
    pub cost_per_1k_output: f64,
}

// ---------------------------------------------------------------------------
// Stream chunk
// ---------------------------------------------------------------------------

/// A single chunk from a streaming completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Incremental text content.
    pub delta: String,
    /// If this chunk completes a tool call, it will be here.
    pub tool_call: Option<ToolCall>,
    /// True if this is the final chunk.
    pub done: bool,
}
