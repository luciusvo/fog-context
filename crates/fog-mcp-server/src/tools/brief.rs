//! fog_brief — server/index status overview.
//!
//! Replaces: health
//! CALL FIRST when starting a task to verify the index is fresh.

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_brief",
        description: "Check fog-context index status: symbol count, file count, last indexed time, \
            schema version, and knowledge layer population (domains, decisions, constraints). \
            MANDATORY FIRST STEP — if symbols = 0, call fog_scan before anything else.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Project path or name. Omit to use current project."
                }
            }
        }),
    }
}

pub fn handle(_args: &Value, db: &MemoryDb) -> ToolCallResult {
    match db.knowledge_score() {
        Ok(score) => {
            let status = if score.total_symbols == 0 {
                "⚠️  NOT INDEXED — call fog_scan first"
            } else if score.layer_score >= 75 {
                "✅ Healthy"
            } else if score.layer_score >= 40 {
                "🟡 Partial — consider adding domains/decisions"
            } else {
                "🔴 Low knowledge quality — run fog_scan + add fog_domains/fog_decisions"
            };

            let mut lines = vec![
                format!("# fog-context Status\n"),
                format!("**State:** {status}"),
                format!("**Knowledge Score:** {}/100", score.layer_score),
                format!("**Symbols:** {}", score.total_symbols),
                format!("**Domains:** {}", score.total_domains),
                format!("**Decisions:** {}", score.total_decisions),
            ];

            if let Some(hint) = score._agent_hint {
                lines.push(format!("\n**Hint:** {hint}"));
            }

            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_brief error: {e}")),
    }
}
