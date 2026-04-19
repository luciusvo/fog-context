//! fog_outline - token-efficient file/directory structure outline.
//! Replaces: skeleton
//! Note: Phase 4A stub - full impl in Phase 4B when indexer is ported.

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_outline",
        description: "Get a lightweight code outline for a file or directory - symbol names, kinds, \
            signatures, and line ranges. ~10x more token-efficient than reading full source files. \
            USE THIS instead of reading files to understand what a module contains.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path or directory prefix relative to project root." },
                "kind": { "type": "string", "description": "Filter by kind (comma-separated): function, method, struct, class, enum." },
                "include_docs": { "type": "boolean", "default": false },
                "max_symbols": { "type": "integer", "default": 100 },
                "project": { "type": "string" }
            },
            "required": ["path"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let path = match args["path"].as_str() {
        Some(p) if !p.is_empty() => p,
        _ => return ToolCallResult::err("fog_outline: 'path' is required"),
    };

    // E5: Root path gives 0 results - explain clearly instead of misleading "not indexed" error
    let is_root = matches!(path, "." | "./" | "/" | "");
    if is_root {
        return ToolCallResult::ok(
            "⚠️  fog_outline does not support root directory - it would return too many symbols.\n\
             Specify a file or subdirectory instead:\n\
             ```\n\
             fog_outline({ \"path\": \"src/\" })          ← outline a directory\n\
             fog_outline({ \"path\": \"src/main.rs\" })   ← outline a single file\n\
             ```\n\
             To search symbols across the whole codebase, use:\n\
             ```\n\
             fog_lookup({ \"query\": \"your_function_name\" })\n\
             ```"
        );
    }

    let kind_filter = args["kind"].as_str();
    let max_symbols = args["max_symbols"].as_u64().unwrap_or(100) as usize;
    let include_docs = args["include_docs"].as_bool().unwrap_or(false);

    // Query symbols by file path prefix from the symbols+files tables
    match db.skeleton(path, max_symbols, kind_filter, include_docs) {
        Ok(symbols) => {
            if symbols.is_empty() {
                // #9: Fuzzy fallback — try partial/suffix path match
                match db.skeleton_fuzzy(path, max_symbols, kind_filter, include_docs) {
                    Ok(fuzzy_hits) if !fuzzy_hits.is_empty() => {
                        let mut lines = vec![format!(
                            "# fog_outline: `{path}` (fuzzy match — {} symbols)\n\
                             > Path matched via suffix/partial search. Exact path not found.\n",
                            fuzzy_hits.len()
                        )];
                        for s in &fuzzy_hits {
                            let doc = if include_docs {
                                s.doc_snippet.as_ref()
                                    .and_then(|d| d.lines().next().map(String::from))
                                    .map(|l| format!("\n  /// {l}"))
                                    .unwrap_or_default()
                            } else { String::new() };
                            lines.push(format!("L{} `{}` [{}]{}\n  → {}  ({})",
                                s.start_line, s.name, s.kind, doc,
                                s.signature.as_deref().unwrap_or("(no signature)"),
                                s.file,
                            ));
                        }
                        return ToolCallResult::ok(lines.join("\n\n"));
                    }
                    _ => {}
                }
                return ToolCallResult::ok(format!(
                    "No symbols found in '{path}'.\n\
                     Check that:\n\
                     1. The path is correct (relative to project root)\n\
                     2. The project is indexed - run fog_scan if unsure\n\
                     3. The file type is supported\n\
                     Try fog_lookup to search by symbol name instead."
                ));
            }

            let mut lines = vec![format!("# fog_outline: `{path}` ({} symbols)\n", symbols.len())];
            for s in &symbols {
                let doc = if include_docs {
                    s.doc_snippet.as_ref()
                        .and_then(|d| d.lines().next().map(String::from))
                        .map(|l| format!("\n  /// {l}"))
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                lines.push(format!("L{} `{}` [{}]{}\n  → {}",
                    s.start_line, s.name, s.kind, doc,
                    s.signature.as_deref().unwrap_or("(no signature)"),
                ));

            }
            ToolCallResult::ok(lines.join("\n\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_outline error: {e}")),
    }
}
