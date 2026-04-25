//! fog_trace - execution flow tracer.
//! Replaces: route_map

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_trace",
        description: "Trace the full execution flow from an entry point. \
            down=trace callees (what it calls), up=trace callers (who calls it). \
            Use token_budget to prevent large codebases from flooding context.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "entry": { "type": "string", "description": "Starting function/method name." },
                "direction": { "type": "string", "enum": ["down", "up"], "default": "down" },
                "depth": { "type": "integer", "description": "Max depth (1-6, default 4)", "default": 4 },
                "token_budget": { "type": "integer", "description": "Max tokens for output." },
                "project": { "type": "string" }
            },
            "required": ["entry"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, project_root: &std::path::Path) -> ToolCallResult {
    let entry = match args["entry"].as_str() {
        Some(e) if !e.is_empty() => e,
        _ => return ToolCallResult::err("fog_trace: 'entry' is required"),
    };
    let direction = args["direction"].as_str().unwrap_or("down");
    let depth = args["depth"].as_u64().unwrap_or(4) as u32;
    let token_budget = args["token_budget"].as_u64().map(|b| b as usize);

    let stale_warn = crate::stale::quick_check(project_root, "fog_trace");

    match db.route_map(entry, depth, direction, token_budget) {
        Ok(result) => {
            let mut lines = vec![format!("{stale_warn}# fog_trace: `{}` ({})\n", entry, direction)];
            if result.nodes.is_empty() {
                return ToolCallResult::ok(format!(
                    "{stale_warn}No call tree for `{entry}`. Check spelling or use fog_lookup first."
                ));
            }
            for node in &result.nodes {
                let indent = "  ".repeat(node.depth as usize);
                let connector = if node.depth == 0 { "▶" } else { "└─" };
                lines.push(format!("{indent}{connector} `{}` [{}] - {}", node.name, node.kind, node.file));
            }
            if result.truncated {
                lines.push(format!(
                    "\n_Truncated ({} tokens). Reduce depth or set token_budget={}._",
                    result.tokens_estimated,
                    result.tokens_estimated * 2,
                ));
            }
            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_trace error: {e}")),
    }
}
