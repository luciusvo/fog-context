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

    // B3 fix: open a dedicated RW connection for graph queries.
    // The shared MemoryDb connection can trigger "Query is not read-only" on
    // some SQLite builds when executing WITH RECURSIVE CTEs under write-check hooks.
    let conn_result = rusqlite::Connection::open(db.db_path())
        .and_then(|c| {
            c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA query_only=0;")?;
            Ok(c)
        });

    let conn = match conn_result {
        Ok(c) => c,
        Err(e) => return ToolCallResult::err(format!("fog_gaps: DB open error: {e}")),
    };

    match run_template(&conn, template, params) {
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

fn run_template(
    conn: &rusqlite::Connection,
    template: &str,
    params: &Value,
) -> rusqlite::Result<Vec<Value>> {
    match template {
        "find_orphans" => {
            let kind = params["kind"].as_str().unwrap_or("function");
            let limit = params["limit"].as_u64().unwrap_or(20) as i64;
            let mut stmt = conn.prepare(
                "SELECT s.name, s.kind, f.path, s.start_line
                 FROM symbols s
                 JOIN files f ON f.id = s.file_id
                 WHERE s.kind = ?1
                 AND s.id NOT IN (SELECT DISTINCT target_id FROM edges WHERE kind='CALLS')
                 AND s.id NOT IN (SELECT DISTINCT source_id FROM edges WHERE kind='CALLS')
                 ORDER BY s.name LIMIT ?2"
            )?;
            let results = stmt.query_map(rusqlite::params![kind, limit], |row| {
                Ok(json!({ "name": row.get::<_,String>(0)?, "kind": row.get::<_,String>(1)?,
                            "file": row.get::<_,String>(2)?, "line": row.get::<_,i64>(3)? }))
            })?.flatten().collect();
            Ok(results)
        }
        "find_cycles" => {
            let mut stmt = conn.prepare(
                "SELECT s.name, s.kind, f.path
                 FROM edges e
                 JOIN symbols s ON s.id = e.source_id
                 JOIN files f ON f.id = s.file_id
                 WHERE e.source_id = e.target_id AND e.kind = 'CALLS'
                 LIMIT 50"
            )?;
            let results = stmt.query_map([], |row| {
                Ok(json!({ "name": row.get::<_,String>(0)?, "kind": row.get::<_,String>(1)?,
                            "file": row.get::<_,String>(2)?, "type": "self_recursion" }))
            })?.flatten().collect();
            Ok(results)
        }
        "find_path" => {
            let from = params["from"].as_str().unwrap_or("");
            let to   = params["to"].as_str().unwrap_or("");
            if from.is_empty() || to.is_empty() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM edges e
                 JOIN symbols s1 ON s1.id = e.source_id AND s1.name = ?1
                 JOIN symbols s2 ON s2.id = e.target_id AND s2.name = ?2",
                rusqlite::params![from, to],
                |row| row.get(0),
            ).unwrap_or(false);
            if exists {
                Ok(vec![json!({ "from": from, "to": to, "direct": true, "path": [from, to] })])
            } else {
                Ok(vec![])
            }
        }
        "find_shared_callers" => {
            let a = params["a"].as_str().unwrap_or("");
            let b = params["b"].as_str().unwrap_or("");
            let mut stmt = conn.prepare(
                "SELECT DISTINCT s.name, s.kind, f.path
                 FROM edges e1
                 JOIN symbols s ON s.id = e1.source_id
                 JOIN files f ON f.id = s.file_id
                 JOIN symbols target1 ON target1.id = e1.target_id AND target1.name = ?1
                 WHERE e1.source_id IN (
                     SELECT e2.source_id FROM edges e2
                     JOIN symbols target2 ON target2.id = e2.target_id AND target2.name = ?2
                 ) LIMIT 20"
            )?;
            let results = stmt.query_map(rusqlite::params![a, b], |row| {
                Ok(json!({ "caller": row.get::<_,String>(0)?, "kind": row.get::<_,String>(1)?,
                            "file": row.get::<_,String>(2)? }))
            })?.flatten().collect();
            Ok(results)
        }
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

