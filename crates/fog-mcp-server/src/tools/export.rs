//! fog_export - Export semantic scope for SAST tools (e.g., Semgrep).

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_export",
        description: "Export semantic scope for SAST tools based on tags.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "format": { "type": "string", "enum": ["semgrep-scope", "paths-only", "json"], "default": "paths-only" },
                "tag_type": { "type": "string", "description": "Filter by tag type (e.g. trust_zone, sink, source, pii)" },
                "tag_value": { "type": "string", "description": "Filter by tag value within the type (e.g. admin, sql)" },
                "project": { "type": "string" }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, _project_root: &std::path::Path) -> ToolCallResult {
    let format = args["format"].as_str().unwrap_or("paths-only");
    let tag_type = args["tag_type"].as_str();
    let tag_value = args["tag_value"].as_str();

    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    match (tag_type, tag_value) {
        (Some(t), Some(v)) => {
            conditions.push(format!("st.tag_type = ?{}", params.len() + 1));
            params.push(Box::new(t.to_string()));
            conditions.push(format!("st.tag_value = ?{}", params.len() + 1));
            params.push(Box::new(v.to_string()));
        }
        (Some(t), None) => {
            conditions.push(format!("st.tag_type = ?{}", params.len() + 1));
            params.push(Box::new(t.to_string()));
        }
        (None, None) => {
            // No filter, get all tagged files
        }
        (None, Some(_)) => {
            return ToolCallResult::err("tag_type is required when tag_value is specified");
        }
    }

    let where_clause = if conditions.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT DISTINCT f.path
         FROM symbol_tags st
         JOIN symbols s ON s.name = st.symbol_name
         JOIN files f ON f.id = s.file_id
         {}
         ORDER BY f.path",
        where_clause
    );

    let conn = db.conn();
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => return ToolCallResult::err(format!("SQL prepare error: {}", e)),
    };

    // Prepare params for rusqlite
    let mut rusqlite_params = Vec::new();
    for p in params.iter() {
        rusqlite_params.push(p.as_ref() as &dyn rusqlite::ToSql);
    }

    let rows = match stmt.query_map(&rusqlite_params[..], |row| row.get::<_, String>(0)) {
        Ok(r) => r,
        Err(e) => return ToolCallResult::err(format!("SQL query error: {}", e)),
    };

    let mut paths = Vec::new();
    for row in rows.flatten() {
        paths.push(row);
    }

    if paths.is_empty() {
        return ToolCallResult::ok("⚠️ No files found matching the specified tag filter.");
    }

    match format {
        "semgrep-scope" => {
            let scope: Vec<String> = paths.iter().map(|p| format!("--include \"{}\"", p)).collect();
            ToolCallResult::ok(scope.join(" "))
        }
        "json" => {
            ToolCallResult::ok(json!(paths).to_string())
        }
        "paths-only" | _ => {
            ToolCallResult::ok(paths.join("\n"))
        }
    }
}
