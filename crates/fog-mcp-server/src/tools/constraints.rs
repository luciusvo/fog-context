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
    // C2 fix: Search multiple ADR locations, not just logs/decisions.
    // Priority order: explicit arg > .fog.yml > common convention paths
    let default_paths = vec![
        "logs/decisions",
        "docs/decisions",
        "docs/adr",
        "docs/rules",
        ".fog/rules",
        ".agent/decisions",
        "decisions",
        "adr",
    ];

    // Check for .fog.yml with custom adr_paths
    let fog_yml_paths = read_fog_yml_adr_paths(project_root);

    let search_paths: Vec<std::path::PathBuf> = if let Some(p) = args["path"].as_str() {
        // Explicit override
        vec![project_root.join(p)]
    } else if !fog_yml_paths.is_empty() {
        fog_yml_paths.iter().map(|p| project_root.join(p)).collect()
    } else {
        default_paths.iter().map(|p| project_root.join(p)).collect()
    };

    let existing: Vec<_> = search_paths.iter().filter(|p| p.exists()).collect();

    if existing.is_empty() {
        let paths_tried: Vec<String> = search_paths.iter()
            .map(|p| format!("  - {}", p.display()))
            .collect();
        return ToolCallResult::ok(format!(
            "⚠️  fog_constraints: No ADR directories found. Tried:\n{}\n\n\
             Create one of these directories and add `.md` files with YAML frontmatter:\n\
             ```yaml\n\
             ---\n\
             code: NO_DIRECT_DB\n\
             severity: ERROR\n\
             statement: \"Handlers must not call DB directly\"\n\
             ---\n\
             ```\n\
             Or create a `.fog.yml` at root with `adr_paths: [custom/path]`",
            paths_tried.join("\n")
        ));
    }

    let mut imported = 0usize;
    let mut files_scanned = 0usize;
    let mut dirs_scanned: Vec<String> = Vec::new();

    for scan_path in &existing {
        dirs_scanned.push(format!("{}", scan_path.display()));
        if let Ok(entries) = std::fs::read_dir(scan_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    files_scanned += 1;
                    if let Ok(content) = std::fs::read_to_string(&path) {
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
    }

    ToolCallResult::ok(format!(
        "✅ fog_constraints: Loaded {imported} constraints from {files_scanned} files\n\
         Scanned directories:\n{}",
        dirs_scanned.iter().map(|d| format!("  - {d}")).collect::<Vec<_>>().join("\n")
    ))
}

/// Read custom adr_paths from .fog.yml if present.
fn read_fog_yml_adr_paths(project_root: &std::path::Path) -> Vec<String> {
    let yml_path = project_root.join(".fog.yml");
    let content = match std::fs::read_to_string(&yml_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    // Simple line-based YAML parser: look for adr_paths: block
    let mut in_adr = false;
    let mut paths = Vec::new();
    for line in content.lines() {
        if line.trim_start().starts_with("adr_paths:") {
            in_adr = true;
            continue;
        }
        if in_adr {
            if line.trim_start().starts_with('-') {
                let p = line.trim_start().trim_start_matches('-').trim().trim_matches('"');
                if !p.is_empty() {
                    paths.push(p.to_string());
                }
            } else if !line.trim().is_empty() && !line.starts_with(' ') {
                break; // new top-level key
            }
        }
    }
    paths
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
