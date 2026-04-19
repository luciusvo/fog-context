//! fog_lookup - symbol search via BM25 FTS5 + centrality ranking.
//! Replaces: search

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_lookup",
        description: "Search project symbols (functions, classes, structs, enums, etc.) \
            using BM25 full-text search weighted by call-graph centrality. \
            Faster and smarter than grep - finds by name, signature, or doc comment. \
            Supports prefix search (query ending with '*') and kind filter.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search term. Use 'run*' for prefix match." },
                "kind": { "type": "string", "description": "Filter: function, method, struct, class, enum, interface, const, type_alias" },
                "limit": { "type": "integer", "description": "Max results (default 30, max 100)", "default": 30 },
                "project": { "type": "string" }
            },
            "required": ["query"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let query = match args["query"].as_str() {
        Some(q) if !q.is_empty() => q,
        _ => return ToolCallResult::err("fog_lookup: 'query' is required"),
    };
    let limit = args["limit"].as_u64().unwrap_or(30) as usize;
    let kind = args["kind"].as_str();

    match db.search(query, limit, kind) {
        Ok(results) => {
            if results.is_empty() {
                return ToolCallResult::ok(format!(
                    "No symbols found for '{query}'. Try fog_outline to browse files."
                ));
            }
            let mut lines = vec![format!("# fog_lookup: '{query}' ({} results)\n", results.len())];
            for r in &results {
                lines.push(format!(
                    "**{}** `{}` - {}\n  📁 {}:{}\n",
                    r.name, r.kind,
                    r.signature.as_deref().unwrap_or(""),
                    r.file, r.start_line,
                ));
            }
            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_lookup error: {e}")),
    }
}
