//! fog_inspect - 360° symbol context view.
//! Replaces: context

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use std::path::Path;
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;
use crate::stale;
use crate::registry::Registry;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_inspect",
        description: "Get 360° context for a symbol: callers, callees, constraints, and decision history. \
            USE THIS before modifying any function - it reveals blast radius and institutional memory.",
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

pub fn handle(args: &Value, db: &MemoryDb, project_root: &Path) -> ToolCallResult {
    let name = match args["name"].as_str() {
        Some(n) if !n.is_empty() => n,
        _ => return ToolCallResult::err("fog_inspect: 'name' is required"),
    };
    let file_hint = args["file"].as_str();

    // #3: Check staleness against last indexed timestamp
    let last_indexed = Registry::load()
        .find(&project_root.to_string_lossy())
        .and_then(|e| e.last_indexed.clone());
    let stale_warn = if let Some(ref f) = file_hint {
        let status = stale::check_stale(project_root, f, last_indexed.as_deref());
        stale::format_warning(&status, "fog_inspect")
    } else { None };

    // #8: Symbol collision disambiguation
    // Count how many symbols share this name before committing to the first one
    let count = db.count_symbols_by_name(name).unwrap_or(0);
    if count >= 2 {
        // If caller provided a file hint, use it to disambiguate
        if file_hint.is_none() {
            // List all matches so AI can choose
            let candidates = db.list_symbols_by_name(name).unwrap_or_default();
            let list = candidates.iter()
                .map(|(f, line)| format!("  - `{name}` at **{f}:{line}**"))
                .collect::<Vec<_>>()
                .join("\n");
            return ToolCallResult::ok(format!(
                "⚠️ **Ambiguous symbol** — {count} definitions named `{name}` found:\n{list}\n\n\
                 Re-call with a file hint to disambiguate:\n\
                 ```\n\
                 fog_inspect({{ \"name\": \"{name}\", \"file\": \"<path from list above>\" }})\n\
                 ```"
            ));
        }
    }

    match db.context_symbol_with_file(name, file_hint) {
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
                    lines.push(format!("- `{}` [{}] - {}", c.name, c.kind, c.file));
                }
            }
            if !ctx.callees.is_empty() {
                lines.push(format!("\n## Callees ({} downstream)", ctx.callees.len()));
                for c in &ctx.callees {
                    lines.push(format!("- `{}` [{}] - {}", c.name, c.kind, c.file));
                }
            }
            if !ctx.decisions.is_empty() {
                lines.push("\n## Decision History".to_string());
                for d in &ctx.decisions {
                    lines.push(format!("- [{}] {} (risk: {})", d.created_at, d.reason, d.revert_risk));
                }
            }
            if !ctx.constraints.is_empty() {
                lines.push("\n## Constraints & Hints".to_string());
                for c in &ctx.constraints {
                    let prefix = if c.code.starts_with("HINT_") { "💡" } else { "🔒" };
                    lines.push(format!("- {} **{}** [{}]: {}", prefix, c.code, c.severity, c.statement));
                }
            }
            let body = lines.join("\n");
            let prefix = stale_warn.unwrap_or_default();
            ToolCallResult::ok(format!("{prefix}{body}"))
        }
        Err(e) => ToolCallResult::err(format!("fog_inspect error: {e}")),
    }
}
