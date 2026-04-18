//! fog_impact — blast radius analysis.
//! Replaces: impact

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_impact",
        description: "Blast radius analysis — shows what breaks if you change a symbol. \
            Returns risk level (LOW/MEDIUM/HIGH/CRITICAL), upstream callers, and downstream deps. \
            ALWAYS run this before modifying a function. HIGH/CRITICAL → warn user.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "description": "Symbol name to analyze." },
                "depth": { "type": "integer", "description": "Max traversal depth (1-5, default 3)", "default": 3 },
                "direction": {
                    "type": "string",
                    "enum": ["upstream", "downstream", "both"],
                    "description": "Default: both",
                    "default": "both"
                },
                "project": { "type": "string" }
            },
            "required": ["target"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let target = match args["target"].as_str() {
        Some(t) if !t.is_empty() => t,
        _ => return ToolCallResult::err("fog_impact: 'target' is required"),
    };
    let depth = args["depth"].as_u64().unwrap_or(3) as u32;
    let direction = args["direction"].as_str().unwrap_or("both");

    match db.impact(target, depth, direction) {
        Ok(result) => {
            let risk_emoji = match result.risk.as_str() {
                "CRITICAL" => "🔴",
                "HIGH"     => "🟠",
                "MEDIUM"   => "🟡",
                _          => "🟢",
            };
            let mut lines = vec![
                format!("# fog_impact: `{target}`\n"),
                format!("**Risk:** {} {} | Upstream: {} | Downstream: {}",
                    risk_emoji, result.risk,
                    result.upstream.len(), result.downstream.len()),
            ];
            if result.risk == "HIGH" || result.risk == "CRITICAL" {
                lines.push(format!(
                    "\n⚠️  **Stop** — risk is {}. Get explicit user approval before editing.", result.risk
                ));
            }
            if let Some(hint) = &result._agent_hint {
                lines.push(format!("\n💡 {hint}"));
            }
            if !result.upstream.is_empty() {
                lines.push(format!("\n## Upstream ({} callers)", result.upstream.len()));
                for s in result.upstream.iter().take(20) {
                    lines.push(format!("  L{} `{}` [{}] — {}", s.depth, s.name, s.kind, s.file));
                }
            }
            if !result.downstream.is_empty() {
                lines.push(format!("\n## Downstream ({} deps)", result.downstream.len()));
                for s in result.downstream.iter().take(20) {
                    lines.push(format!("  L{} `{}` [{}] — {}", s.depth, s.name, s.kind, s.file));
                }
            }
            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_impact error: {e}")),
    }
}
