//! fog_decisions - log WHY a code change was made.
//! Replaces: record_decision

use std::path::Path;

use fog_memory::{MemoryDb, write::RecordDecisionArgs};
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_decisions",
        description: "Record WHY you changed code. Call this AFTER completing changes. \
            Builds institutional memory that future AI sessions read via fog_inspect. \
            MANDATORY after every significant architectural or behavioral change.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "functions": { "type": "array", "items": { "type": "string" }, "description": "Function names changed." },
                "reason": { "type": "string", "description": "Why this change was made." },
                "domain": { "type": "string", "description": "Business domain (optional)." },
                "revert_risk": { "type": "string", "enum": ["LOW", "MEDIUM", "HIGH"], "default": "LOW" },
                "project": { "type": "string" }
            },
            "required": ["functions", "reason"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, project_root: &Path) -> ToolCallResult {
    let functions: Vec<String> = match args["functions"].as_array() {
        Some(a) if !a.is_empty() => a.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => return ToolCallResult::err("fog_decisions: 'functions' is required"),
    };
    let reason = match args["reason"].as_str() {
        Some(r) if !r.is_empty() => r,
        _ => return ToolCallResult::err("fog_decisions: 'reason' is required"),
    };

    // ── 2-Tier symbol validation ──────────────────────────────────────────────
    // Tier 1: DB lookup (fast path - preferred when index is fresh)
    // Open direct connection since MemoryDb::conn() is pub(crate)
    let db_match = rusqlite::Connection::open(db.db_path()).ok().map(|conn| {
        functions.iter().any(|f| {
            conn.query_row(
                "SELECT 1 FROM symbols WHERE name = ?1 LIMIT 1",
                rusqlite::params![f],
                |_| Ok(true),
            ).unwrap_or(false)
        })
    }).unwrap_or(false);

    // Tier 2: Filesystem grep fallback (for when DB is empty/stale after parse failures)
    // Uses ripgrep (rg) if available, falls back to grep -r.
    let (validated, validation_note) = if db_match {
        (true, String::new())
    } else {
        let found_on_disk = functions.iter().any(|f| {
            // Try rg first (faster), then grep
            let result = std::process::Command::new("rg")
                .args(["--max-count=1", "--no-heading", "-l", f])
                .arg(project_root)
                .output()
                .or_else(|_| std::process::Command::new("grep")
                    .args(["-r", "-l", f])
                    .arg(project_root)
                    .output()
                )
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(false);
            result
        });

        if found_on_disk {
            (true, "\n> ⚠️ Symbol not in index (run fog_scan). Validated via filesystem.".to_string())
        } else {
            (false, "\n> ❓ Symbol not found in index or filesystem. Check function names.".to_string())
        }
    };
    // ─────────────────────────────────────────────────────────────────────────

    let decision_args = RecordDecisionArgs {
        functions: functions.clone(),
        reason: reason.to_string(),
        domain: args["domain"].as_str().map(String::from),
        revert_risk: args["revert_risk"].as_str().map(String::from),
        supersedes_id: None,
    };

    match db.record_decision(decision_args) {
        Ok(id) => ToolCallResult::ok(format!(
            "✅ Decision recorded (id={id}, validated={validated}).\n\
            Functions: {}\nReason: {reason}\n\
            Visible in fog_inspect for all listed functions.{validation_note}",
            functions.join(", "),
        )),
        Err(e) => ToolCallResult::err(format!("fog_decisions error: {e}")),
    }
}

