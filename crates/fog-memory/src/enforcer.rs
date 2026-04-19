//! fog-memory/enforcer.rs - Gateway Loop Interceptors
//!
//! The Enforcer is the **active enforcement** component of Track 2.
//! It intercepts agent tool calls via fog-harness/gateway_loop and
//! automatically triggers required memory-maintenance actions.
//!
//! ## Enforcement Rules (R6 - Zero Trust)
//!
//! | Trigger | Action |
//! |:--------|:-------|
//! | Agent calls `record_decision` MCP tool | Re-route to native `MemoryDb::record_decision()` |
//! | Agent calls `define_domain` MCP tool   | Re-route to native `MemoryDb::define_domain()` |
//! | Agent calls `scratchpad` MCP tool      | Re-route to native `MemoryDb::scratchpad_*()` |
//! | 5+ tool calls since last decision      | Inject nudge into system prompt |
//! | File modified (editor event)           | Trigger auto-record_decision prompt |
//!
//! ## Design
//!
//! The Enforcer is **read-side-free** - it never blocks the agent's original request.
//! It acts *in addition to* (not instead of) the agent's intended action.
//!
//! PATTERN_DECISION: Level 4 (Simple Class)
//! Justification: Enforcer needs stateful call counter (`tool_calls_since_decision`)
//! that persists across gateway_loop turns within a session.

use std::sync::{Arc, Mutex};

use crate::{
    db::MemoryDb,
    write::{RecordDecisionArgs, ScratchpadUpdateArgs},
    MemoryResult,
};

// ---------------------------------------------------------------------------
// EnforcerAction - what the interceptor detected + decided
// ---------------------------------------------------------------------------

/// Classification of an incoming agent tool call.
#[derive(Debug, Clone, PartialEq)]
pub enum EnforcerAction {
    /// Normal tool call - no special enforcement needed.
    Passthrough,
    /// Tool call should be mirrored to the native MemoryDb.
    Mirror { tool: String, payload: String },
    /// Agent hasn't called record_decision in too long - inject nudge.
    InjectNudge { message: String },
    /// A file was changed - agent should call record_decision.
    AutoCommit { hint: String },
}

/// Result of the interceptor analysis.
#[derive(Debug, Clone)]
pub struct EnforcerResponse {
    pub action: EnforcerAction,
    /// Optional text to prepend to the next system prompt injection.
    pub prompt_injection: Option<String>,
    /// If true, the gateway_loop should delay the response until after the action.
    pub blocking: bool,
}

// ---------------------------------------------------------------------------
// Enforcer - stateful interceptor
// ---------------------------------------------------------------------------

/// Session-scoped enforcer that tracks agent behavior across turns.
///
/// PATTERN_DECISION: Level 4 (Simple Class)
/// Purpose: counter state across calls. Constructor takes Arc<Mutex<MemoryDb>> for DI.
/// `Mutex<MemoryDb>` is Sync even when MemoryDb itself is not (rusqlite limitation).
pub struct Enforcer {
    db: Arc<Mutex<MemoryDb>>,
    /// Calls since the last `record_decision` was made.
    tool_calls_since_decision: Mutex<u32>,
    /// Threshold before nudge injection.
    nudge_threshold: u32,
}

impl Enforcer {
    /// Create a new Enforcer for a session.
    ///
    /// `nudge_threshold`: number of tool calls before nudge. Default: 5.
    pub fn new(db: Arc<Mutex<MemoryDb>>, nudge_threshold: Option<u32>) -> Self {
        Self {
            db,
            tool_calls_since_decision: Mutex::new(0),
            nudge_threshold: nudge_threshold.unwrap_or(5),
        }
    }

    /// Intercept a tool call. Returns an `EnforcerResponse` describing what to do.
    ///
    /// Called by `gateway_loop::run_turn()` before dispatching the tool call.
    ///
    /// PATTERN_DECISION: Level 2 (Composition of sub-checks)
    pub fn intercept(&self, tool_name: &str, payload: &str) -> EnforcerResponse {
        let mut count = self.tool_calls_since_decision.lock().unwrap();
        *count += 1;

        // --- 1. Mirror memory-maintenance tool calls to native DB ---
        if let Some(action) = self.mirror_if_memory_tool(tool_name, payload) {
            *count = 0; // Reset timer when agent proactively calls memory tools
            return EnforcerResponse {
                action,
                prompt_injection: None,
                blocking: false,
            };
        }

        // --- 2. Nudge if agent hasn't maintained memory in a while ---
        if *count >= self.nudge_threshold {
            let msg = format!(
                "⚠️  Memory Maintenance Due: {count} tool calls since last record_decision. \
                 Consider calling record_decision() to log your reasoning (R4 - FoG protocol).",
            );
            return EnforcerResponse {
                action: EnforcerAction::InjectNudge { message: msg.clone() },
                prompt_injection: Some(msg),
                blocking: false,
            };
        }

        // --- 3. Normal passthrough ---
        EnforcerResponse {
            action: EnforcerAction::Passthrough,
            prompt_injection: None,
            blocking: false,
        }
    }

    /// Execute a mirrored action against the native DB.
    ///
    /// Called by gateway_loop when `EnforcerAction::Mirror` is returned.
    pub fn execute_mirror(&self, tool: &str, payload: &str) -> MemoryResult<()> {
        let db = self.db.lock().unwrap();
        match tool {
            "record_decision" => {
                let args: RecordDecisionArgs = serde_json::from_str(payload)
                    .map_err(crate::MemoryError::Json)?;
                db.record_decision(args)?;
            }
            "scratchpad_update" => {
                // Payload: { role: string, state: ScratchpadUpdateArgs }
                #[derive(serde::Deserialize)]
                struct ScratchpadMirrorPayload {
                    role: String,
                    #[serde(flatten)]
                    state: ScratchpadUpdateArgs,
                }
                let p: ScratchpadMirrorPayload = serde_json::from_str(payload)
                    .map_err(crate::MemoryError::Json)?;
                db.scratchpad_update(&p.role, p.state)?;
            }
            _ => {
                tracing::debug!(tool, "fog-memory: unknown mirror tool, skipping");
            }
        }
        Ok(())
    }

    /// Called by gateway_loop when a file is saved (editor → telemetry event).
    ///
    /// Generates a prompt injection nudging the agent to explain the change.
    pub fn on_file_saved(&self, file_path: &str) -> EnforcerResponse {
        let hint = format!(
            "📝 File saved: `{file_path}`. \
             If you modified this file intentionally, call record_decision(functions=[...], reason=\"...\") \
             to log WHY. This feeds Layer 4 (Causality) for future AI sessions.",
        );
        EnforcerResponse {
            action: EnforcerAction::AutoCommit { hint: hint.clone() },
            prompt_injection: Some(hint),
            blocking: false,
        }
    }

    /// Reset the tool-call counter (e.g., after agent successfully records a decision).
    pub fn reset_counter(&self) {
        *self.tool_calls_since_decision.lock().unwrap() = 0;
    }

    // ---------------------------------------------------------------------------
    // Private
    // ---------------------------------------------------------------------------

    fn mirror_if_memory_tool(&self, tool: &str, payload: &str) -> Option<EnforcerAction> {
        match tool {
            "record_decision" | "scratchpad_update" | "define_domain" => {
                Some(EnforcerAction::Mirror {
                    tool: tool.to_string(),
                    payload: payload.to_string(),
                })
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::open_test_db;

    fn make_enforcer() -> Enforcer {
        let db = Arc::new(Mutex::new(open_test_db()));
        Enforcer::new(db, Some(3)) // low threshold for testing
    }

    #[test]
    fn passthrough_for_normal_tool() {
        let enf = make_enforcer();
        let resp = enf.intercept("search", r#"{"query":"gateway"}"#);
        assert_eq!(resp.action, EnforcerAction::Passthrough);
        assert!(resp.prompt_injection.is_none());
    }

    #[test]
    fn mirror_for_record_decision() {
        let enf = make_enforcer();
        let payload = r#"{"functions":["foo"],"reason":"test"}"#;
        let resp = enf.intercept("record_decision", payload);
        assert!(matches!(resp.action, EnforcerAction::Mirror { .. }));
    }

    #[test]
    fn nudge_after_threshold() {
        let enf = make_enforcer(); // threshold = 3
        enf.intercept("search", "{}");
        enf.intercept("search", "{}");
        enf.intercept("search", "{}");
        let resp = enf.intercept("search", "{}"); // 4th call → nudge
        assert!(matches!(resp.action, EnforcerAction::InjectNudge { .. }));
        assert!(resp.prompt_injection.is_some());
    }

    #[test]
    fn mirror_resets_counter() {
        let enf = make_enforcer(); // threshold = 3
        enf.intercept("search", "{}");
        enf.intercept("search", "{}");
        // Mirror call resets counter
        let payload = r#"{"functions":["foo"],"reason":"test"}"#;
        enf.intercept("record_decision", payload);
        // Counter reset - should not nudge immediately
        let resp = enf.intercept("search", "{}");
        assert_eq!(resp.action, EnforcerAction::Passthrough);
    }

    #[test]
    fn on_file_saved_returns_auto_commit() {
        let enf = make_enforcer();
        let resp = enf.on_file_saved("src/gateway.rs");
        assert!(matches!(resp.action, EnforcerAction::AutoCommit { .. }));
        assert!(resp.prompt_injection.unwrap().contains("gateway.rs"));
    }

    #[test]
    fn execute_mirror_record_decision() {
        let enf = make_enforcer();
        let payload = r#"{"functions":["test_fn"],"reason":"Testing mirror","domain":null,"revert_risk":"LOW","supersedes_id":null}"#;
        let result = enf.execute_mirror("record_decision", payload);
        assert!(result.is_ok());
    }
}
