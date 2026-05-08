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
                "project": { "type": "string" },
                "suggest_domains": { "type": "boolean", "default": false, "description": "Scan folders to suggest L2 domains" },
                "seed_git": { "type": "boolean", "default": false, "description": "Scan Git history to seed draft L4 decisions" }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, project_root: &Path) -> ToolCallResult {
    let fog_context_dir = project_root.join(".fog-context");
    
    if !fog_context_dir.exists() {
        if let Err(e) = fs::create_dir_all(&fog_context_dir) {
            return ToolCallResult::err(format!("Failed to create .fog-context directory: {}", e));
        }
    }

    let security_path = fog_context_dir.join("security.toml");
    let labels_path = fog_context_dir.join("labels.toml");

    let suggest = args.get("suggest_domains").and_then(Value::as_bool).unwrap_or(false);
    let seed = args.get("seed_git").and_then(Value::as_bool).unwrap_or(false);
    let mut output = String::new();

    if !suggest && !seed {
        if security_path.exists() || labels_path.exists() {
            return ToolCallResult::ok("Configurations already exist. Please modify them manually or delete them to bootstrap again.");
        }
    }

    if !security_path.exists() && !labels_path.exists() {

    // Helper to query actual symbols matching a pattern
    let get_symbols = |pattern: &str| -> Vec<String> {
        db.conn()
            .prepare("SELECT name FROM symbols WHERE name LIKE ? LIMIT 10")
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

    output.push_str("Successfully generated default security.toml and labels.toml. Please review them, then run `fog_overlay` to apply tags.\n");
    }

    if suggest {
        match run_suggest_domains(db) {
            Ok(res) => output.push_str(&res),
            Err(e) => return ToolCallResult::err(format!("suggest_domains failed: {}", e)),
        }
    }

    if seed {
        match run_seed_git(db, project_root) {
            Ok(res) => output.push_str(&res),
            Err(e) => return ToolCallResult::err(format!("seed_git failed: {}", e)),
        }
    }

    ToolCallResult::ok(output)
}

fn run_suggest_domains(db: &MemoryDb) -> Result<String, String> {
    let query = "
        SELECT
            CASE
                WHEN f.path LIKE 'src/%' THEN SUBSTR(f.path, 5, INSTR(SUBSTR(f.path, 5), '/') - 1)
                WHEN f.path LIKE 'crates/%' THEN SUBSTR(f.path, 8, INSTR(SUBSTR(f.path, 8), '/') - 1)
                WHEN f.path LIKE 'packages/%' THEN SUBSTR(f.path, 10, INSTR(SUBSTR(f.path, 10), '/') - 1)
                WHEN f.path LIKE 'lib/%' THEN SUBSTR(f.path, 5, INSTR(SUBSTR(f.path, 5), '/') - 1)
                ELSE SUBSTR(f.path, 1, INSTR(f.path, '/') - 1)
            END AS module_name,
            COUNT(DISTINCT s.id) as symbol_count,
            COUNT(DISTINCT f.id) as file_count
        FROM files f
        JOIN symbols s ON s.file_id = f.id
        GROUP BY module_name
        HAVING module_name IS NOT NULL AND module_name != ''
        ORDER BY symbol_count DESC LIMIT 20";

    let mut stmt = db.conn().prepare(query).map_err(|e| format!("Database error: {}", e))?;
    let mut rows = stmt.query([]).map_err(|e| format!("Query error: {}", e))?;

    let mut table = String::from("## Domain Suggestions\n\n| Module | Files | Symbols | Suggested Command |\n|---|---|---|---|\n");
    let mut found = false;

    while let Some(row) = rows.next().map_err(|e| format!("Row error: {}", e))? {
        found = true;
        let module_name: String = row.get(0).unwrap_or_default();
        let symbol_count: i64 = row.get(1).unwrap_or(0);
        let file_count: i64 = row.get(2).unwrap_or(0);
        
        let command = format!("`fog_assign({{ \"domain\": \"{}\", \"keywords\": [\"{}\"] }})`", module_name, module_name);
        table.push_str(&format!("| {} | {} | {} | {} |\n", module_name, file_count, symbol_count, command));
    }

    if !found {
        return Ok("No distinct modules found to suggest domains.\n".to_string());
    }

    table.push_str("\n> ⚠️ Files directly in src/ are not grouped into any module.\n\n");
    Ok(table)
}

fn run_seed_git(db: &MemoryDb, project_root: &Path) -> Result<String, String> {
    let git_out = std::process::Command::new("git")
        .args(["log", "--oneline", "--no-merges", "-n", "50", "--format=%H|%s|%an|%ai"])
        .current_dir(project_root)
        .output().map_err(|e| format!("Failed to run git: {}", e))?;

    if !git_out.status.success() {
        return Err(format!("git command failed: {}", String::from_utf8_lossy(&git_out.stderr)));
    }

    let output_str = String::from_utf8_lossy(&git_out.stdout);
    let mut seeded = 0;

    let mut stmt = db.conn().prepare("SELECT name FROM symbols WHERE ?1 LIKE '%' || name || '%' AND LENGTH(name) >= 8 LIMIT 1")
        .map_err(|e| format!("DB prepare error: {}", e))?;

    for line in output_str.lines() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 4 { continue; }
        
        let hash = parts[0];
        let subject = parts[1].trim();
        if subject.is_empty() { continue; }
        
        let matched_symbol: Option<String> = stmt.query_row([subject], |row| row.get(0)).ok();

        if let Some(sym) = matched_symbol {
            if let Err(e) = db.record_decision(fog_memory::write::RecordDecisionArgs {
                functions: vec![sym],
                reason: format!("{} ({})", subject, hash),
                status: Some("seed".to_string()),
                ..Default::default()
            }) {
                tracing::warn!("Failed to seed decision for commit {}: {}", hash, e);
                continue;
            }
            seeded += 1;
        }
    }

    Ok(format!("## Git History Seed\nSeeded {} draft decisions from Git history.\n\n", seeded))
}
