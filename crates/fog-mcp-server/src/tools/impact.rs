//! fog_impact - blast radius analysis.
//! Replaces: impact

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use std::path::Path;
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;
use crate::stale;
use crate::registry::Registry;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_impact",
        description: "Blast radius analysis - shows what breaks if you change a symbol. \
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

pub fn handle(args: &Value, db: &MemoryDb, project_root: &Path) -> ToolCallResult {
    let target = match args["target"].as_str() {
        Some(t) if !t.is_empty() => t,
        _ => return ToolCallResult::err("fog_impact: 'target' is required"),
    };
    let depth = args["depth"].as_u64().unwrap_or(3) as u32;
    let direction = args["direction"].as_str().unwrap_or("both");

    // #3: Stale detection — check if target's file changed since last scan
    let last_indexed = Registry::load()
        .find(&project_root.to_string_lossy())
        .and_then(|e| e.last_indexed.clone());
    let stale_status = stale::check_stale(project_root, target, last_indexed.as_deref());
    let stale_warn = stale::format_warning(&stale_status, "fog_impact");

    // #4: Hard cap to prevent context window flooding
    const MAX_NODES: usize = 100;

    match db.impact(target, depth, direction) {
        Ok(result) => {
            let risk_emoji = match result.risk.as_str() {
                "CRITICAL" => "🔴",
                "HIGH"     => "🟠",
                "MEDIUM"   => "🟡",
                _          => "🟢",
            };
            let total_up = result.upstream.len();
            let total_down = result.downstream.len();
            let mut lines = vec![
                format!("# fog_impact: `{target}`\n"),
                format!("**Risk:** {} {} | Upstream: {} | Downstream: {}",
                    risk_emoji, result.risk, total_up, total_down),
            ];
            if result.risk == "HIGH" || result.risk == "CRITICAL" {
                lines.push(format!(
                    "\n⚠️  **Stop** - risk is {}. Get explicit user approval before editing.", result.risk
                ));
            }
            if matches!(result.risk.as_str(), "MEDIUM" | "HIGH" | "CRITICAL") {
                lines.push(format!(
                    "\n> 📝 **MANDATORY:** Risk is {}. You MUST call `fog_decisions` after making changes to record WHY you edited this.", result.risk
                ));
            }
            if let Some(hint) = &result._agent_hint {
                lines.push(format!("\n💡 {hint}"));
            }
            if !result.upstream.is_empty() {
                let shown = result.upstream.len().min(MAX_NODES);
                lines.push(format!("\n## Upstream ({} callers{})",
                    total_up,
                    if total_up > MAX_NODES { format!(" — showing {MAX_NODES}/{total_up}") } else { String::new() }
                ));
                for s in result.upstream.iter().take(MAX_NODES) {
                    lines.push(format!("  L{} `{}` [{}] - {}", s.depth, s.name, s.kind, s.file));
                }
                if total_up > MAX_NODES {
                    lines.push(format!(
                        "\n> [!WARNING]\n> **Truncated:** {}/{} nodes shown. Use `depth=1` for a tighter scope, or `direction=\"upstream\"` to focus.",
                        shown, total_up
                    ));
                }
            }
            if !result.downstream.is_empty() {
                let shown = result.downstream.len().min(MAX_NODES);
                lines.push(format!("\n## Downstream ({} deps{})",
                    total_down,
                    if total_down > MAX_NODES { format!(" — showing {MAX_NODES}/{total_down}") } else { String::new() }
                ));
                for s in result.downstream.iter().take(MAX_NODES) {
                    lines.push(format!("  L{} `{}` [{}] - {}", s.depth, s.name, s.kind, s.file));
                }
                if total_down > MAX_NODES {
                    lines.push(format!(
                        "\n> [!WARNING]\n> **Truncated:** {}/{} nodes shown. Use `depth=1` for a tighter scope.",
                        shown, total_down
                    ));
                }
            }
            let body = lines.join("\n");
            let prefix = stale_warn.unwrap_or_default();
            ToolCallResult::ok(format!("{prefix}{body}"))
        }
        Err(e) => ToolCallResult::err(format!("fog_impact error: {e}")),
    }
}
