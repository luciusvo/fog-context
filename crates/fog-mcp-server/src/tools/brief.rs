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

pub fn handle(_args: &Value, db: &MemoryDb, registry: &crate::registry::Registry) -> ToolCallResult {
    match db.knowledge_score() {
        Ok(score) => {
            let status = if score.total_symbols == 0 {
                "⚠️  NOT INDEXED — call fog_scan first"
            } else if score.layer_score >= 75 {
                "✅ Healthy"
            } else if score.layer_score >= 40 {
                "🟡 Partial — see knowledge gaps below"
            } else {
                "🔴 Low knowledge quality"
            };

            let mut lines = vec![
                format!("# fog-context Status\n"),
                format!("**State:** {status}"),
                format!("**Knowledge Score:** {}/100", score.layer_score),
                format!("**Symbols:** {} across {} files", score.total_symbols, score.total_files),
                format!("**Edges:** {}", score.total_edges),
                format!("**Domains (L2):** {}", score.total_domains),
                format!("**Constraints (L3):** {}", score.total_constraints),
                format!("**Decisions (L4):** {}", score.total_decisions),
                format!("**Projects in registry:** {}", registry.list().len()),
            ];

            // Sprint 3D: mandatory enforcement reminder for empty layers
            if score.total_symbols > 0 {
                let mut gaps: Vec<String> = Vec::new();

                if score.total_domains == 0 {
                    gaps.push(
                        "  → **Layer 2 empty** — group symbols into domains:\n\
                         ```\n\
                         fog_assign({ \"domain\": \"YourFeature\", \"symbols\": [\"fn_a\", \"fn_b\"] })\n\
                         ```".to_string()
                    );
                }
                if score.total_constraints == 0 {
                    gaps.push(
                        "  → **Layer 3 empty** — load architecture rules from ADR files:\n\
                         ```\n\
                         fog_constraints({})\n\
                         ```".to_string()
                    );
                }
                if score.total_decisions == 0 {
                    gaps.push(
                        "  → **⚠️  Layer 4 EMPTY** — no decisions recorded. MANDATORY after every change:\n\
                         ```\n\
                         fog_decisions({ \"functions\": [\"changed_fn\"], \"reason\": \"WHY\", \"revert_risk\": \"LOW\" })\n\
                         ```".to_string()
                    );
                }

                if !gaps.is_empty() {
                    lines.push(format!("\n## 🔴 Knowledge Gaps (Action Required)\n{}", gaps.join("\n\n")));
                }
            }

            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_brief error: {e}")),
    }
}

