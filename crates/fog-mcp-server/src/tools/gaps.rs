//! fog_gaps — graph analysis: find cycles, orphans, communities.
//! Replaces: graph_query

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_gaps",
        description: "Advanced graph analysis using safe pre-defined query templates. \
            Find architectural gaps: circular dependencies (find_cycles), dead code (find_orphans), \
            call coupling (find_shared_callers), or direct connections (find_path).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "template": {
                    "type": "string",
                    "enum": ["find_cycles", "find_orphans", "find_shared_callers", "find_path"],
                    "description": "Analysis template."
                },
                "params": {
                    "type": "object",
                    "description": "find_cycles: {} | find_orphans: {kind?, limit?} | find_shared_callers: {a, b} | find_path: {from, to}"
                },
                "project": { "type": "string" }
            },
            "required": ["template"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let template = match args["template"].as_str() {
        Some(t) => t,
        None => return ToolCallResult::err("fog_gaps: 'template' is required"),
    };
    let params = &args["params"];

    match db.graph_query(template, params) {
        Ok(results) => {
            let mut lines = vec![format!("# fog_gaps: {template}\n")];
            if results.is_empty() {
                lines.push(format!("✅ No issues found for '{template}' — graph looks clean."));
            } else {
                lines.push(format!("Found {} result(s):\n", results.len()));
                for r in &results {
                    lines.push(format!("- {}", serde_json::to_string(r).unwrap_or_default()));
                }
            }
            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_gaps error: {e}")),
    }
}
