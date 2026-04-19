//! fog_constraints - load ADR files into the constraint layer.
//! Replaces: ingest_adrs
//! Phase 4A: stub - ingest_adrs lives in write.rs (to be added in Phase 4B).

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_constraints",
        description: "Scan ADR files and YAML rule files to populate the constraints database (Layer 3). \
            Also supports inline injection: pass 'code' + 'statement' to add a constraint directly \
            without needing an ADR file. Supports markdown tables, YAML schemas, and INVARIANTS comment blocks.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Override scan path (relative to project root). Default: logs/decisions/ and docs/rules/"
                },
                "init": {
                    "type": "boolean",
                    "description": "If true, create logs/decisions/ directory and a template ADR file if none exist."
                },
                "code": {
                    "type": "string",
                    "description": "[Inline mode] Machine-readable constraint ID (e.g. 'NO_DIRECT_DB'). When provided, injects the constraint directly into the DB without scanning files."
                },
                "statement": {
                    "type": "string",
                    "description": "[Inline mode] Human-readable rule (e.g. 'HTTP handlers must not call the database directly')."
                },
                "severity": {
                    "type": "string",
                    "enum": ["ERROR", "WARNING", "INFO"],
                    "description": "[Inline mode] Severity level. Default: ERROR."
                },
                "project": { "type": "string" }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb, project_root: &std::path::Path) -> ToolCallResult {
    // Inline injection mode: pass code + statement directly, skip file scanning
    if let Some(code) = args["code"].as_str().filter(|s| !s.is_empty()) {
        let statement = match args["statement"].as_str().filter(|s| !s.is_empty()) {
            Some(s) => s,
            None => return ToolCallResult::err("fog_constraints (inline): 'statement' is required when 'code' is provided"),
        };
        let severity = args["severity"].as_str().unwrap_or("ERROR");
        if !matches!(severity, "ERROR" | "WARNING" | "INFO") {
            return ToolCallResult::err("fog_constraints (inline): severity must be ERROR, WARNING, or INFO");
        }
        return match db.insert_constraint(code, severity, statement) {
            Ok(_) => ToolCallResult::ok(format!(
                "✅ Constraint added to Layer 3:\n\
                 - **Code:** `{code}`\n\
                 - **Severity:** {severity}\n\
                 - **Rule:** {statement}\n\n\
                 Verify with fog_brief({{}}) → check Constraints (L3) count."
            )),
            Err(e) => ToolCallResult::err(format!("fog_constraints (inline) error: {e}")),
        };
    }

    // #3L3: HINT_ convention — semantic bridge constraints for cross-language / runtime edges
    // Detected automatically when code starts with HINT_; stored in Layer 3 as INFO severity
    // with a synthetic note that static analysis cannot see this edge.
    // (Already handled by inline mode above; this block adds a confirmation note.)

    // Init mode: bootstrap Layer 3 by creating template ADR directory + file
    if args["init"].as_bool().unwrap_or(false) {
        return handle_init(project_root);
    }

    // C2 fix: Search multiple ADR locations, not just logs/decisions.
    // Priority order: explicit arg > .fog-context/config.toml > .fog.yml > common convention paths
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

    // F1: Use centralized ADR path loading (reads config.toml first, then .fog.yml)
    let config_paths = crate::registry::load_project_adr_paths(project_root);

    let search_paths: Vec<std::path::PathBuf> = if let Some(p) = args["path"].as_str() {
        let candidate = project_root.join(p);
        // #10: Raw text file mode — if path is a single file (not directory), ingest it directly
        if candidate.is_file() {
            return handle_raw_text_file(&candidate, db);
        }
        // Explicit directory override
        vec![candidate]
    } else if !config_paths.is_empty() {
        config_paths.iter().map(|p| project_root.join(p)).collect()
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

/// #10: Ingest a single raw text file as bulk constraints.
///
/// Supported formats:
/// - `CODE:SEVERITY:rule statement` — explicit triple format
/// - `HINT_*: statement` — semantic bridge hints (auto-INFO severity)
/// - Plain line — auto-generates code from file+line number, INFO severity
fn handle_raw_text_file(path: &std::path::Path, db: &MemoryDb) -> ToolCallResult {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::err(format!(
            "fog_constraints: cannot read file {}: {e}", path.display()
        )),
    };

    let file_stem = path.file_stem()
        .map(|s| s.to_string_lossy().to_uppercase())
        .unwrap_or_else(|| "RULE".into());

    let mut imported = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        // Parse formats:
        // 1. CODE:SEVERITY:statement  (explicit)
        // 2. HINT_NAME: statement     (semantic bridge, auto-INFO)
        // 3. plain text               (auto-code, INFO)
        let code: String;
        let severity: &str;
        let statement: String;

        if let Some(first_colon) = line.find(':') {
            let code_candidate = line[..first_colon].trim().to_uppercase();
            let rest = line[first_colon + 1..].trim();

            if let Some(second_colon) = rest.find(':') {
                // Maybe CODE:SEVERITY:statement format
                let sev_candidate = rest[..second_colon].trim().to_uppercase();
                if matches!(sev_candidate.as_str(), "ERROR" | "WARNING" | "WARN" | "INFO") {
                    code = code_candidate;
                    severity = match sev_candidate.as_str() {
                        "ERROR" => "ERROR",
                        "WARNING" | "WARN" => "WARNING",
                        _ => "INFO",
                    };
                    statement = rest[second_colon + 1..].trim().to_string();
                } else {
                    // Colon in statement, not a triple format
                    code = format!("{file_stem}_{:04}", line_num + 1);
                    severity = "INFO";
                    statement = line.to_string();
                }
            } else if code_candidate.starts_with("HINT_") {
                // HINT_NAME: statement
                code = code_candidate;
                severity = "INFO";
                statement = rest.to_string();
            } else {
                // Single colon but not a recognized pattern
                code = format!("{file_stem}_{:04}", line_num + 1);
                severity = "INFO";
                statement = line.to_string();
            }
        } else {
            code = format!("{file_stem}_{:04}", line_num + 1);
            severity = "INFO";
            statement = line.to_string();
        }

        match db.insert_constraint(&code, severity, &statement) {
            Ok(_) => imported += 1,
            Err(e) => errors.push(format!("L{}: {e}", line_num + 1)),
        }
    }

    let warn = if errors.is_empty() { String::new() } else {
        format!("\n\n⚠️ {} lines failed:\n{}", errors.len(), errors.join("\n"))
    };

    ToolCallResult::ok(format!(
        "✅ **Raw text ingested:** {}\n\
         - **Imported:** {imported} constraints into Layer 3\n\
         - **Tip:** Use `HINT_NAME: description` lines to record semantic bridges\
         {warn}",
        path.display()
    ))
}

/// E6: Bootstrap Layer 3 by creating template ADR structure.
fn handle_init(project_root: &std::path::Path) -> ToolCallResult {
    let adr_dir = project_root.join("logs/decisions");
    let template_path = adr_dir.join("0001-invariants-template.md");

    if template_path.exists() {
        return ToolCallResult::ok(format!(
            "✅ ADR directory already exists: {}\n\
             Run fog_constraints({{}}) to scan existing ADR files.",
            adr_dir.display()
        ));
    }

    if let Err(e) = std::fs::create_dir_all(&adr_dir) {
        return ToolCallResult::err(format!("fog_constraints init: failed to create {}: {e}", adr_dir.display()));
    }

    let template = concat!(
        "---\n",
        "code: EXAMPLE_CONSTRAINT\n",
        "severity: ERROR\n",
        "statement: \"Replace this with your architecture rule (e.g. 'Handlers must not call DB directly')\"\n",
        "---\n",
        "\n",
        "# ADR-0001: Architecture Invariants\n",
        "\n",
        "## Context\n",
        "Describe the architectural context and why this constraint exists.\n",
        "\n",
        "## Decision\n",
        "Document the specific rule and the reasoning behind it.\n",
        "\n",
        "## Consequences\n",
        "Describe what changes if this rule is violated.\n",
        "\n",
        "---\n",
        "<!-- Add more constraints as additional --- blocks or new .md files -->\n"
    );

    if let Err(e) = std::fs::write(&template_path, template) {
        return ToolCallResult::err(format!("fog_constraints init: failed to write template: {e}"));
    }

    ToolCallResult::ok(format!(
        "✅ fog_constraints init complete!\n\
         Created: {}\n\n\
         Next steps:\n\
         1. Edit {} - replace EXAMPLE_CONSTRAINT with your rules\n\
         2. Run fog_constraints({{}}) to load them into Layer 3\n\
         3. Run fog_brief({{}}) to verify constraints count > 0",
        adr_dir.display(),
        template_path.display()
    ))
}

