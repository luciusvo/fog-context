//! fog_inspect — 360° symbol context view.
//! Replaces: context

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_inspect",
        description: "Get 360° context for a symbol: callers, callees, constraints, and decision history. \
            USE THIS before modifying any function — it reveals blast radius and institutional memory.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Symbol name to inspect." },
                "file": { "type": "string", "description": "File path hint if name is ambiguous." },
                "project": { "type": "string" }
            },
            "required": ["name"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let name = match args["name"].as_str() {
        Some(n) if !n.is_empty() => n,
        _ => return ToolCallResult::err("fog_inspect: 'name' is required"),
    };

    match db.context_symbol(name) {
        Ok(None) => ToolCallResult::ok(format!(
            "Symbol '{name}' not found. Use fog_lookup to search by name first."
        )),
        Ok(Some(ctx)) => {
            let mut lines = vec![
                format!("# fog_inspect: `{}`\n", ctx.name),
                format!("**Kind:** {} | **File:** {}:{}", ctx.kind, ctx.file, ctx.start_line),
            ];
            if let Some(sig) = &ctx.signature {
                lines.push(format!("\n```\n{sig}\n```"));
            }
            if !ctx.callers.is_empty() {
                lines.push(format!("\n## Callers ({} upstream)", ctx.callers.len()));
                for c in &ctx.callers {
                    lines.push(format!("- `{}` [{}] — {}", c.name, c.kind, c.file));
                }
            }
            if !ctx.callees.is_empty() {
                lines.push(format!("\n## Callees ({} downstream)", ctx.callees.len()));
                for c in &ctx.callees {
                    lines.push(format!("- `{}` [{}] — {}", c.name, c.kind, c.file));
                }
            }
            if !ctx.decisions.is_empty() {
                lines.push("\n## Decision History".to_string());
                for d in &ctx.decisions {
                    lines.push(format!("- [{}] {} (risk: {})", d.created_at, d.reason, d.revert_risk));
                }
            }
            if !ctx.constraints.is_empty() {
                lines.push("\n## Constraints".to_string());
                for c in &ctx.constraints {
                    lines.push(format!("- **{}** [{}]: {}", c.code, c.severity, c.statement));
                }
            }
            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_inspect error: {e}")),
    }
}
