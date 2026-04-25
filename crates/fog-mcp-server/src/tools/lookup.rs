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

pub fn handle(args: &Value, db: &MemoryDb, project_root: &std::path::Path) -> ToolCallResult {
    let query = match args["query"].as_str() {
        Some(q) if !q.is_empty() => q,
        _ => return ToolCallResult::err("fog_lookup: 'query' is required"),
    };
    let limit = args["limit"].as_u64().unwrap_or(30) as usize;
    let kind = args["kind"].as_str();

    let stale_warn = crate::stale::quick_check(project_root, "fog_lookup");

    match db.search(query, 100, kind) {
        Ok(mut results) => {
            if results.is_empty() {
                return ToolCallResult::ok(format!(
                    "{stale_warn}No symbols found for '{query}'. Try fog_outline to browse files."
                ));
            }

            #[cfg(feature = "embedding")]
            {
                if let Some(_model) = crate::semantic::get_model() {
                    if let Ok(query_vector) = crate::semantic::embed_text(query) {
                        let ids: Vec<i64> = results.iter().map(|r| r.id).collect();
                        if let Ok(embeddings) = db.fetch_symbol_embeddings(&ids) {
                            let mut embed_map = std::collections::HashMap::new();
                            for (id, vector) in embeddings {
                                embed_map.insert(id, vector);
                            }
                            
                            for hit in &mut results {
                                if let Some(vector) = embed_map.get(&hit.id) {
                                    let similarity = crate::semantic::cosine_similarity(&query_vector, vector);
                                    hit.relevance = (hit.relevance * 0.6) + (similarity as f64 * 0.4);
                                }
                            }
                            results.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));
                        }
                    }
                }
            }

            results.truncate(limit);

            let mut lines = vec![format!("{stale_warn}# fog_lookup: '{query}' ({} results)\n", results.len())];
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
