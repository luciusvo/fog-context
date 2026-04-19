//! Unified error types for the FoG toolchain.
//!
//! Every crate wraps its internal errors into domain-specific variants,
//! but all share these common escalation statuses from the FoG methodology.

use serde::{Deserialize, Serialize};
use std::fmt;

/// FoG escalation status - signals the system's constraint layer is active.
///
/// These are NOT failures in the traditional sense. They are signals that
/// the AI has reached a boundary and needs human intervention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscalationStatus {
    /// AI lacks required context (glossary, contracts, tier info).
    /// Human action: provide missing context.
    MissingContext { reason: String },

    /// AI failed 3 times on same problem. Issue is likely in spec/interface, not code.
    /// Human action: review design, not implementation.
    Magic { reason: String },

    /// Operation requires human approval before proceeding (T3 / irreversible).
    /// Human action: approve or reject.
    AwaitingHumanApproval { operation: String },
}

impl fmt::Display for EscalationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingContext { reason } => {
                write!(f, "ESCALATE_MISSING_CONTEXT: {reason}")
            }
            Self::Magic { reason } => {
                write!(f, "ESCALATE_MAGIC: {reason}")
            }
            Self::AwaitingHumanApproval { operation } => {
                write!(f, "AWAITING_HUMAN_APPROVAL: {operation}")
            }
        }
    }
}

/// Core error type shared across all crates.
///
/// Each crate defines its own error enum but can convert into this
/// for cross-crate error propagation.
#[derive(Debug, thiserror::Error)]
pub enum FogError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("Tool execution error: {tool} - {reason}")]
    ToolExecution { tool: String, reason: String },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Escalation: {0}")]
    Escalation(EscalationStatus),

    #[error("Not found: {entity} with id '{id}'")]
    NotFound { entity: String, id: String },

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Result type alias used throughout the toolchain.
pub type FogResult<T> = Result<T, FogError>;
