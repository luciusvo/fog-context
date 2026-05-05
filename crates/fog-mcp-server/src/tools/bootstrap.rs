//! fog-mcp-server/src/tools/bootstrap.rs
//!
//! Generates default security.toml and labels.toml configs and updates AGENTS.md.
//! Queries existing symbols to provide accurate default patterns.
//!
//! PATTERN_DECISION: Level 1 (Pure Function)
//! Justification: File generation and simple DB querying logic.

use std::fs;
use std::path::Path;

use fog_memory::MemoryDb;
use serde_json::{json, Value};

use crate::protocol::{ToolCallResult, ToolDef};
use crate::indexer::ingest;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_bootstrap",
        description: "Generate default security.toml and labels.toml configurations. Updates AGENTS.md.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "project": { "type": "string" }
            }
        }),
    }
}

pub fn handle(_args: &Value, db: &MemoryDb, project_root: &Path) -> ToolCallResult {
    let fog_context_dir = project_root.join(".fog-context");
    
    if !fog_context_dir.exists() {
        if let Err(e) = fs::create_dir_all(&fog_context_dir) {
            return ToolCallResult::err(format!("Failed to create .fog-context directory: {}", e));
        }
    }

    let security_path = fog_context_dir.join("security.toml");
    let labels_path = fog_context_dir.join("labels.toml");

    if security_path.exists() || labels_path.exists() {
        return ToolCallResult::ok("Configurations already exist. Please modify them manually or delete them to bootstrap again.");
    }

    // Helper to query actual symbols matching a pattern
    let get_symbols = |pattern: &str| -> Vec<String> {
        db.conn()
            .prepare("SELECT name FROM symbols WHERE name LIKE ?")
            .and_then(|mut stmt| {
                let rows: Result<Vec<String>, _> = stmt.query_map([pattern], |r| r.get(0))?.collect();
                rows
            })
            .unwrap_or_default()
    };

    let sql_symbols = get_symbols("%sqlx%");
    let http_symbols = get_symbols("%reqwest%");

    let sql_defaults = if sql_symbols.is_empty() {
        "\"sqlx_query\", \"sqlx_execute\"".to_string()
    } else {
        sql_symbols.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ")
    };

    let http_defaults = if http_symbols.is_empty() {
        "\"reqwest_get\", \"reqwest_post\"".to_string()
    } else {
        http_symbols.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ")
    };

    let security_content = format!(
        "# ⚠️ Auto-generated from symbol analysis. Review before use.\n\
         # Replace with your actual function names if these are incorrect.\n\
         \n\
         [sources]\n\
         # http = [\"axum::extract\", \"actix_web::get\"]\n\
         \n\
         [sinks]\n\
         sql = [{}]\n\
         http = [{}]\n\
         \n\
         [sanitizers]\n\
         # html = [\"ammonia::clean\"]\n",
        sql_defaults, http_defaults
    );

    let labels_content = "# ⚠️ Auto-generated from symbol analysis. Review before use.\n\
         \n\
         [pii]\n\
         # email = [\"get_email\", \"User::email\"]\n\
         \n\
         [credentials]\n\
         # tokens = [\"Jwt::decode\"]\n\
         \n\
         [trust_zones]\n\
         # admin = [\"is_admin\"]\n".to_string();

    if let Err(e) = fs::write(&security_path, security_content) {
        return ToolCallResult::err(format!("Failed to write security.toml: {}", e));
    }
    if let Err(e) = fs::write(&labels_path, labels_content) {
        return ToolCallResult::err(format!("Failed to write labels.toml: {}", e));
    }

    // Call write_agents_md to ensure rule is created
    let symbol_count = db.conn().query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get::<_, usize>(0)).unwrap_or(0);
    ingest::write_agents_md(project_root, 0, symbol_count);

    ToolCallResult::ok("Successfully generated default security.toml and labels.toml. Please review them, then run `fog_overlay` to apply tags.")
}
