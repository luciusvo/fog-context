//! Configuration types for .fog-autoide.toml.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Root configuration — deserialized from `.fog-autoide.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FogConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub languages: LanguageConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    /// fog-context MCP server integration (M1).
    /// When present, enables semantic graph tools inside `fog task`.
    #[serde(default)]
    pub fog_context: FogContextConfig,
}

/// Configuration for the fog-context MCP server integration.
///
/// fog-context gives the agent a SQLite semantic graph of the codebase,
/// replacing naive grep-based search with BM25 + centrality ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FogContextConfig {
    /// Absolute or relative path to `dist/index.js` of the fog-context server.
    /// When None (default), auto-detection is attempted in this order:
    ///   1. $FOG_CONTEXT_SERVER env var
    ///   2. Binary-relative `../MCP/fog-context/dist/index.js`
    /// Set explicitly to disable auto-detect: `server_path = "/path/to/dist/index.js"`
    #[serde(default)]
    pub server_path: Option<String>,
    /// Whether to enable fog-context integration (default: true).
    /// Set to false to disable even if server_path is found.
    #[serde(default = "default_fog_context_enabled")]
    pub enabled: bool,
}

impl Default for FogContextConfig {
    fn default() -> Self {
        Self {
            server_path: None,
            enabled: true,
        }
    }
}

fn default_fog_context_enabled() -> bool {
    true
}

/// Project metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    /// Default risk tier for new modules (0-3).
    #[serde(default = "default_tier")]
    pub default_tier: u8,
    /// Path to .fog/ directory (default: ".fog").
    #[serde(default = "default_fog_dir")]
    pub fog_dir: PathBuf,
}

/// Language parsing settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// Which languages to parse (default: all supported).
    #[serde(default = "default_languages")]
    pub enabled: Vec<String>,
    /// File patterns to exclude from indexing.
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

// ===========================================================================
// LLM Configuration (Smart API Gateway)
// ===========================================================================

/// LLM provider settings — multi-provider, multi-account Smart API Gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default model in "provider/model" format (e.g., "gemini/gemini-2.5-flash").
    #[serde(default = "default_model")]
    pub default_model: String,
    /// Ordered fallback chain: when active model is rate-limited, try next.
    #[serde(default)]
    pub fallback_chain: Vec<String>,
    /// Max retries on same model before falling back to next.
    #[serde(default = "default_max_retries")]
    pub max_retries_before_fallback: u32,
    /// Cooldown in seconds for a rate-limited model before retrying it.
    #[serde(default = "default_cooldown_secs")]
    pub cooldown_secs: u64,
    /// Budget controls for cost management.
    #[serde(default)]
    pub budget: BudgetConfig,
    /// Model selection strategy.
    #[serde(default)]
    pub strategy: StrategyConfig,
    /// Per-provider configurations.
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

/// Configuration for a single LLM provider.
///
/// Supports both legacy single-key (`api_key_env`) and multi-account pool (`accounts`).
/// If both are specified, `accounts` takes precedence.
/// If only `api_key_env` is specified, it's auto-converted to a single-account pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Legacy: single API key env var. Auto-converted to accounts[0] if accounts is empty.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Custom base URL (for local providers: Ollama, LM Studio, vLLM).
    #[serde(default)]
    pub base_url: Option<String>,
    /// API format: "gemini", "anthropic", "openai", "ollama".
    /// Inferred from provider name if not specified.
    #[serde(default)]
    pub api_format: Option<String>,
    /// Account pool — multiple API keys for quota rotation.
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
    /// Models available under this provider.
    #[serde(default)]
    pub models: Vec<ModelConfig>,
}

impl ProviderConfig {
    /// Get effective accounts: if accounts[] is empty but api_key_env is set,
    /// auto-create a single-account pool for backward compatibility.
    pub fn effective_accounts(&self) -> Vec<AccountConfig> {
        if !self.accounts.is_empty() {
            return self.accounts.clone();
        }
        if let Some(ref key_env) = self.api_key_env {
            vec![AccountConfig {
                name: "default".into(),
                api_key_env: key_env.clone(),
                priority: 1,
            }]
        } else {
            // Local provider — no API key needed
            vec![AccountConfig {
                name: "local".into(),
                api_key_env: String::new(),
                priority: 1,
            }]
        }
    }
}

/// A single API account (API key) for quota rotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    /// Human-readable name (e.g., "primary", "secondary", "free-tier").
    pub name: String,
    /// Environment variable holding the API key.
    #[serde(default)]
    pub api_key_env: String,
    /// Lower = preferred. Account with lowest priority used first.
    #[serde(default = "default_priority")]
    pub priority: u32,
}

/// Configuration for a single model under a provider.
///
/// Plug & Play: for models not in the static catalog, specify overrides here.
/// The system will merge config overrides with catalog defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model identifier (e.g., "gemini-2.5-flash").
    pub id: String,
    /// Whether this model is available for selection.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Optional display name override.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Override max input context tokens.
    #[serde(default)]
    pub max_input_tokens: Option<usize>,
    /// Override max output tokens.
    #[serde(default)]
    pub max_output_tokens: Option<usize>,
    /// Override cost per 1K input tokens (USD).
    #[serde(default)]
    pub cost_per_1k_input: Option<f64>,
    /// Override cost per 1K output tokens (USD).
    #[serde(default)]
    pub cost_per_1k_output: Option<f64>,
    /// Override tool calling support.
    #[serde(default)]
    pub supports_tools: Option<bool>,
}

/// Budget configuration for cost management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Max cost in USD per single session (0.0 = unlimited).
    #[serde(default)]
    pub max_cost_per_session: f64,
    /// Max cost in USD per day (0.0 = unlimited).
    #[serde(default)]
    pub max_cost_per_day: f64,
    /// Warning threshold as fraction of limit (e.g., 0.8 = warn at 80%).
    #[serde(default = "default_warn_threshold")]
    pub warn_threshold: f64,
}

// ===========================================================================
// Model Selection Strategy
// ===========================================================================

/// Strategy for selecting which model handles a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// "manual" | "auto_by_tier" | "advisor"
    #[serde(default = "default_strategy_mode")]
    pub mode: String,
    /// Advisor strategy settings (only used when mode = "advisor").
    #[serde(default)]
    pub advisor: AdvisorConfig,
}

/// Advisor pattern config: cheap executor + expensive advisor on escalation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorConfig {
    /// Executor model ref (cheap/fast). Default: same as llm.default_model.
    #[serde(default)]
    pub executor: Option<String>,
    /// Advisor model ref (expensive/smart). Consulted when executor is stuck.
    #[serde(default)]
    pub advisor: Option<String>,
    /// Max advisor consultations per task session.
    #[serde(default = "default_max_consults")]
    pub max_consults_per_task: u32,
}

// ===========================================================================
// Request Context (tracking dimensions)
// ===========================================================================

/// Context for each LLM request — carries tracking metadata through the SAG.
#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    /// Session ID (one `fog task` invocation).
    pub session_id: String,
    /// Conversation ID (groups multi-turn within a session).
    pub conversation_id: String,
    /// Turn number within this conversation.
    pub turn_number: u32,
    /// Whether this request was triggered by a failover.
    pub is_failover: bool,
    /// Whether this request is an advisor consultation.
    pub is_advisor: bool,
}

// ===========================================================================
// Impl blocks
// ===========================================================================

impl LlmConfig {
    /// List all enabled models as "provider/model" strings.
    pub fn enabled_models(&self) -> Vec<String> {
        let mut models = Vec::new();
        for (provider_name, config) in &self.providers {
            for model in &config.models {
                if model.enabled {
                    models.push(format!("{}/{}", provider_name, model.id));
                }
            }
        }
        models
    }

    /// Check if a "provider/model" ref is valid and enabled.
    pub fn is_model_enabled(&self, model_ref: &str) -> bool {
        if let Some((provider, model_id)) = model_ref.split_once('/') {
            if let Some(config) = self.providers.get(provider) {
                return config.models.iter().any(|m| m.id == model_id && m.enabled);
            }
        }
        false
    }

    /// Get provider config for a "provider/model" ref.
    pub fn provider_for_model<'a>(&'a self, model_ref: &'a str) -> Option<(&'a str, &'a ProviderConfig, &'a str)> {
        if let Some((provider, model_id)) = model_ref.split_once('/') {
            if let Some(config) = self.providers.get(provider) {
                if config.models.iter().any(|m| m.id == model_id) {
                    return Some((provider, config, model_id));
                }
            }
        }
        None
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            fallback_chain: Vec::new(),
            max_retries_before_fallback: default_max_retries(),
            cooldown_secs: default_cooldown_secs(),
            budget: BudgetConfig::default(),
            strategy: StrategyConfig::default(),
            providers: HashMap::new(),
        }
    }
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_cost_per_session: 0.0,
            max_cost_per_day: 0.0,
            warn_threshold: default_warn_threshold(),
        }
    }
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            mode: default_strategy_mode(),
            advisor: AdvisorConfig::default(),
        }
    }
}

impl Default for AdvisorConfig {
    fn default() -> Self {
        Self {
            executor: None,
            advisor: None,
            max_consults_per_task: default_max_consults(),
        }
    }
}

/// MCP server settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// Transport type (currently only "stdio").
    #[serde(default = "default_transport")]
    pub transport: String,
    /// Path to SQLite database.
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_tier() -> u8 {
    1
}

fn default_fog_dir() -> PathBuf {
    PathBuf::from(".fog")
}

fn default_languages() -> Vec<String> {
    vec![
        "typescript".into(),
        "python".into(),
        "rust".into(),
    ]
}

fn default_model() -> String {
    "gemini/gemini-2.5-flash".into()
}

fn default_enabled() -> bool {
    true
}

fn default_max_retries() -> u32 {
    2
}

fn default_cooldown_secs() -> u64 {
    60
}

fn default_warn_threshold() -> f64 {
    0.8
}

fn default_strategy_mode() -> String {
    "manual".into()
}

fn default_max_consults() -> u32 {
    3
}

fn default_priority() -> u32 {
    1
}

fn default_transport() -> String {
    "stdio".into()
}

fn default_db_path() -> PathBuf {
    PathBuf::from(".fog/fog.db")
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            enabled: default_languages(),
            exclude_patterns: vec![
                "node_modules/**".into(),
                "target/**".into(),
                ".git/**".into(),
                "__pycache__/**".into(),
                "dist/**".into(),
                "build/**".into(),
            ],
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            transport: default_transport(),
            db_path: default_db_path(),
        }
    }
}
