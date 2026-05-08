//! fog_gaps - graph analysis: find cycles, orphans, communities.
//! Replaces: graph_query

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_gaps",
        description: "Advanced graph analysis using safe pre-defined query templates. \
            Find architectural gaps: circular dependencies (find_cycles), dead code (find_dead_code), \
            call coupling (find_shared_callers), or direct connections (find_path).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "template": {
                    "type": "string",
                    "enum": ["find_cycles", "find_dead_code", "find_shared_callers", "find_path"],
                    "description": "Analysis template."
                },
                "params": {
                    "type": "object",
                    "description": "find_cycles: {} | find_dead_code: {kind?, limit?} | find_shared_callers: {a, b} | find_path: {from, to}"
                },
                "project": { "type": "string" }
            },
            "required": ["template"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, project_root: &std::path::Path) -> ToolCallResult {
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

    let stale_warn = crate::stale::quick_check(project_root, "fog_gaps");

    match run_template(&conn, template, params) {
        Ok(results) => {
            let mut lines = vec![format!("{stale_warn}# fog_gaps: {template}\n")];
            if results.is_empty() {
                lines.push(format!("✅ No issues found for '{template}' - graph looks clean."));
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
        "find_dead_code" => {
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
        "find_cycles" => tarjan_find_cycles(conn),
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
        "find_orphans" => {
            run_template(conn, "find_dead_code", params)
        }
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn tarjan_find_cycles(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Value>> {
    use std::collections::{HashMap, HashSet};

    let edges: Vec<(i64, i64)> = conn.prepare(
        "SELECT source_id, target_id FROM edges WHERE kind = 'CALLS'"
    )?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
      ?.flatten().collect();

    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for (u, v) in edges {
        adj.entry(u).or_default().push(v);
    }

    let mut index = 0;
    let mut indices: HashMap<i64, usize> = HashMap::new();
    let mut lowlink: HashMap<i64, usize> = HashMap::new();
    let mut on_stack: HashSet<i64> = HashSet::new();
    let mut stack: Vec<i64> = Vec::new();
    let mut sccs: Vec<Vec<i64>> = Vec::new();

    fn strongconnect(
        v: i64,
        index: &mut usize,
        indices: &mut HashMap<i64, usize>,
        lowlink: &mut HashMap<i64, usize>,
        on_stack: &mut HashSet<i64>,
        stack: &mut Vec<i64>,
        adj: &HashMap<i64, Vec<i64>>,
        sccs: &mut Vec<Vec<i64>>,
    ) {
        indices.insert(v, *index);
        lowlink.insert(v, *index);
        *index += 1;
        stack.push(v);
        on_stack.insert(v);

        if let Some(neighbors) = adj.get(&v) {
            for &w in neighbors {
                if !indices.contains_key(&w) {
                    strongconnect(w, index, indices, lowlink, on_stack, stack, adj, sccs);
                    let low_v = *lowlink.get(&v).expect("Tarjan invariant: v missing in lowlink");
                    let low_w = *lowlink.get(&w).expect("Tarjan invariant: w missing in lowlink");
                    lowlink.insert(v, low_v.min(low_w));
                } else if on_stack.contains(&w) {
                    let low_v = *lowlink.get(&v).expect("Tarjan invariant: v missing in lowlink");
                    let index_w = *indices.get(&w).expect("Tarjan invariant: w missing in indices");
                    lowlink.insert(v, low_v.min(index_w));
                }
            }
        }

        if lowlink.get(&v) == indices.get(&v) {
            let mut scc = Vec::new();
            loop {
                let w = stack.pop().expect("Tarjan invariant: stack empty before root found");
                on_stack.remove(&w);
                scc.push(w);
                if w == v {
                    break;
                }
            }
            sccs.push(scc);
        }
    }

    for &v in adj.keys() {
        if !indices.contains_key(&v) {
            strongconnect(v, &mut index, &mut indices, &mut lowlink, &mut on_stack, &mut stack, &adj, &mut sccs);
        }
    }

    let mut cycle_nodes = HashSet::new();
    let mut cycle_components = Vec::new();
    for scc in sccs {
        if scc.len() > 1 || (scc.len() == 1 && adj.get(&scc[0]).map_or(false, |ns| ns.contains(&scc[0]))) {
            for &node in &scc {
                cycle_nodes.insert(node);
            }
            cycle_components.push(scc);
        }
    }

    if cycle_nodes.is_empty() {
        return Ok(Vec::new());
    }

    let mut symbol_map: HashMap<i64, Value> = HashMap::new();
    let nodes: Vec<i64> = cycle_nodes.into_iter().collect();
    for chunk in nodes.chunks(900) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let query = format!("SELECT s.id, s.name, s.kind, f.path FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.id IN ({})", placeholders);
        let mut stmt = conn.prepare(&query)?;
        let params = rusqlite::params_from_iter(chunk.iter());
        let rows = stmt.query_map(params, |row| {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let path: String = row.get(3)?;
            Ok((id, json!({"name": name, "kind": kind, "file": path})))
        })?;
        for row in rows {
            if let Ok((id, val)) = row {
                symbol_map.insert(id, val);
            }
        }
    }

    let mut results = Vec::new();
    for scc in cycle_components {
        let mut path = Vec::new();
        for node in scc {
            if let Some(info) = symbol_map.get(&node) {
                path.push(info.clone());
            }
        }
        results.push(json!({
            "type": "cycle",
            "size": path.len(),
            "nodes": path
        }));
    }

    Ok(results)
}
