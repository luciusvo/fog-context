//! Event types for the internal event bus.
//!
//! Events are the heartbeat of the agentic system. Every significant action
//! emits an event, enabling logging, telemetry, and hook-based extensibility.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::llm::TokenUsage;

/// An event emitted during agentic execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub kind: EventKind,
}

/// What happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// Session lifecycle
    SessionStarted {
        task: String,
        model_id: String,
    },
    SessionCompleted {
        outcome: String,
        total_turns: u32,
    },

    /// LLM interaction
    LlmRequestSent {
        model_id: String,
        estimated_tokens: usize,
    },
    LlmResponseReceived {
        model_id: String,
        usage: TokenUsage,
        stop_reason: String,
    },

    /// Tool execution
    ToolExecuted {
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },

    /// FoG-specific events
    EscalationRaised {
        status: String,
        reason: String,
    },
    ContextCompacted {
        tokens_before: usize,
        tokens_after: usize,
    },
    /// Fired when context usage crosses the 80% compaction threshold.
    /// fog.session.compaction_events - AEGIS L4 Lifecycle Gate metric.
    ContextCompactionTriggered {
        /// Integer percentage (0–100).
        context_pct: u32,
        /// Current number of messages in the window.
        messages_count: u32,
    },
    CircuitBroken {
        consecutive_failures: u32,
        last_error: String,
    },
    ProviderSwitched {
        from_model: String,
        to_model: String,
    },
    /// Fired when ≥4 consecutive identical tool calls are detected.
    /// fog.session.doom_loops - AEGIS L4 Lifecycle Gate metric.
    /// Triggers immediate ESCALATE_MAGIC per FoG R5.
    DoomLoopDetected {
        /// The tool being called repeatedly.
        tool_name: String,
        /// Number of consecutive identical calls observed.
        consecutive_count: u32,
    },

    /// Indexing events
    FileIndexed {
        path: String,
        symbols_found: u32,
    },
    IndexCompleted {
        files_processed: u32,
        total_symbols: u32,
        duration_ms: u64,
    },

    /// AEGIS tier enforcement events (M3)
    ///
    /// Emitted when a tool call is warned (T2) or blocked (T3) by the
    /// AEGIS guard. Both appear in the EventBus and are written to the
    /// anti-blackbox audit log (R6 compliance).
    AegisWarning {
        /// The tool call that triggered the warning.
        tool_name: String,
        /// Risk tier of the module being modified.
        tier: u8,
        /// Human-readable warning message injected into agent conversation.
        message: String,
    },
    AegisBlocked {
        /// The tool call that was blocked.
        tool_name: String,
        /// Risk tier that caused the block.
        tier: u8,
        /// Reason for blocking - injected as AEGIS_BLOCK error into conversation.
        reason: String,
    },

    /// fog-context semantic tool called (M1 telemetry).
    FogContextToolCalled {
        /// fog-context tool name (e.g. "search", "impact", "route_map").
        tool_name: String,
        /// Whether the call succeeded.
        success: bool,
        /// Latency in ms.
        duration_ms: u64,
    },

    // -----------------------------------------------------------------------
    // T1-C: Streaming events - real-time feedback during LLM turns
    // -----------------------------------------------------------------------

    /// A chunk of streamed text from the LLM (T1-C).
    ///
    /// Emitted for EACH chunk received from the provider's streaming API.
    /// Subscribers accumulate chunks to reconstruct the full response text.
    /// UI/CLI consumers use this for live typewriter-style display.
    TokenDelta {
        /// The incremental text chunk from the stream.
        delta: String,
        /// Which turn this chunk belongs to (1-indexed).
        turn: u32,
        /// Cumulative character count in the current response so far.
        chars_so_far: usize,
    },

    /// Emitted after each full LLM turn with running cost totals (T1-C).
    ///
    /// Enables real-time cost dashboards. The `cumulative_cost_usd` field
    /// lets any subscriber display a live "$0.012 spent" indicator without
    /// reading from the ledger file.
    TurnCostUpdated {
        /// Which turn just completed (1-indexed).
        turn: u32,
        /// Tokens consumed in this turn.
        input_tokens: u32,
        output_tokens: u32,
        /// Cumulative USD cost across ALL turns so far in this session.
        cumulative_cost_usd: f64,
        /// Estimated remaining budget (if a budget was set; None otherwise).
        remaining_budget_usd: Option<f64>,
    },

    /// A transient streaming error - not fatal, may recover (T1-C).
    ///
    /// Fired when a chunk is corrupted or the stream stalls temporarily.
    /// The agentic loop retries (up to circuit-breaker limit) on this event.
    StreamError {
        /// Human-readable error description.
        error: String,
        /// Which turn encountered the error.
        turn: u32,
        /// Whether recovery will be attempted.
        will_retry: bool,
    },
}

impl Event {
    /// Create a new event with current timestamp.
    pub fn now(session_id: impl Into<String>, kind: EventKind) -> Self {
        Self {
            timestamp: Utc::now(),
            session_id: session_id.into(),
            kind,
        }
    }
}
