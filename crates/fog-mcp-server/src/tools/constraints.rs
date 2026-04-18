//! fog_constraints — load ADR files into the constraint layer.
//! Replaces: ingest_adrs
//! Phase 4A: stub — ingest_adrs lives in write.rs (to be added in Phase 4B).

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_constraints",
        description: "Scan ADR files and YAML rule files to populate the constraints database. \
            Run this once after adding new ADRs or invariant definitions. \
            Supports markdown tables, YAML schemas, and INVARIANTS comment blocks.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Override scan path (relative to project root). Default: logs/decisions/ and docs/rules/"
                },
                "project": { "type": "string" }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, project_root: &std::path::Path) -> ToolCallResult {
    let scan_path = args["path"].as_str()
        .map(|p| project_root.join(p))
        .unwrap_or_else(|| project_root.join("logs/decisions"));

    if !scan_path.exists() {
        return ToolCallResult::ok(format!(
            "⚠️  fog_constraints: Path '{}' not found.\n\
             Create ADR files in logs/decisions/ or docs/rules/ with YAML frontmatter:\n\
             ```yaml\n\
             ---\n\
             code: NO_DIRECT_DB\n\
             severity: ERROR\n\
             statement: \"Handlers must not call DB directly\"\n\
             ---\n\
             ```",
            scan_path.display()
        ));
    }

    // Scan markdown files for YAML frontmatter constraints
    let mut imported = 0usize;
    let mut files_scanned = 0usize;

    if let Ok(entries) = std::fs::read_dir(&scan_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                files_scanned += 1;
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Parse YAML frontmatter between --- delimiters
                    if let Some(constraints) = parse_adr_constraints(&content) {
                        for (code, severity, statement) in constraints {
                            match db.insert_constraint(&code, &severity, &statement) {
                                Ok(_) => imported += 1,
                                Err(e) => tracing::warn!("fog_constraints: skip {code}: {e}"),
                            }
                        }
                    }
                }
            }
        }
    }

    ToolCallResult::ok(format!(
        "✅ fog_constraints: Loaded {imported} constraints from {files_scanned} files in {}",
        scan_path.display()
    ))
}

/// Parse YAML frontmatter from an ADR markdown file.
/// Looks for: code, severity, statement fields.
fn parse_adr_constraints(content: &str) -> Option<Vec<(String, String, String)>> {
    let frontmatter = content.strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))?;
    let fm = frontmatter.0;

    let get_field = |key: &str| -> Option<String> {
        fm.lines()
            .find(|l| l.starts_with(&format!("{key}:")))
            .map(|l| l[key.len() + 1..].trim().trim_matches('"').to_string())
    };

    let code = get_field("code")?;
    let severity = get_field("severity").unwrap_or_else(|| "INFO".to_string());
    let statement = get_field("statement")?;

    Some(vec![(code, severity, statement)])
}
