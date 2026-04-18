//! fog_scan — index or re-index the codebase using Tree-sitter.
//! Replaces: index (fog-context TS)

use std::path::Path;
use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_scan",
        description: "Index or re-index the codebase using Tree-sitter AST parsing. \
            Extracts symbols (functions, structs, classes, enums) and call graph edges. \
            Incremental by default — only re-parses changed files (uses XXH3 checksums). \
            Run once before using other tools, or after large code changes. \
            Use 'full=true' to force complete re-index.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project path to index. Defaults to current project." },
                "full": { "type": "boolean", "default": false, "description": "Force full re-index (ignores checksums)." }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, project_root: &Path) -> ToolCallResult {
    let full = args["full"].as_bool().unwrap_or(false);

    // Delegate to the two-pass Tree-sitter indexer
    crate::indexer::run_scan(project_root, db, full)
}
