//! fog-mcp-server/src/tools/overlay.rs
//!
//! Applies security and label overlays from `.fog-context/security.toml` and `labels.toml`.
//! Generates tags in the `symbol_tags` table for matching symbols.
//!
//! PATTERN_DECISION: Level 1 (Pure Function)
//! Justification: The configuration parsing and DB execution are straight-line pipelines.

use std::collections::HashMap;
use std::path::Path;

use fog_memory::MemoryDb;
use serde_json::{json, Value};

use crate::protocol::{ToolCallResult, ToolDef};

#[derive(Default)]
pub struct SecurityConfig {
    pub sources: HashMap<String, Vec<String>>,
    pub sinks: HashMap<String, Vec<String>>,
    pub sanitizers: HashMap<String, Vec<String>>,
}

#[derive(Default)]
pub struct LabelsConfig {
    pub pii: HashMap<String, Vec<String>>,
    pub credentials: HashMap<String, Vec<String>>,
    pub trust_zones: HashMap<String, Vec<String>>,
}

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_overlay",
        description: "Apply security and label overlays from configuration to project symbols. Run this to refresh symbol_tags after modifying security.toml or labels.toml.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Project ID to apply overlays to."
                }
            }
        }),
    }
}

pub fn handle(_args: &Value, db: &MemoryDb, project_root: &Path) -> ToolCallResult {
    let fog_context_dir = project_root.join(".fog-context");
    let security_path = fog_context_dir.join("security.toml");
    let labels_path = fog_context_dir.join("labels.toml");

    if !security_path.exists() && !labels_path.exists() {
        return ToolCallResult::ok(
            "⚠️ Configuration not found. You can generate default overlays by running `fog_bootstrap` or create `.fog-context/security.toml` manually.",
        );
    }

    let mut security_cfg = SecurityConfig::default();
    if security_path.exists() {
        let content = std::fs::read_to_string(&security_path).unwrap_or_default();
        let parsed = parse_simple_toml(&content);
        security_cfg.sources = parsed.get("sources").cloned().unwrap_or_default();
        security_cfg.sinks = parsed.get("sinks").cloned().unwrap_or_default();
        security_cfg.sanitizers = parsed.get("sanitizers").cloned().unwrap_or_default();
    }

    let mut labels_cfg = LabelsConfig::default();
    if labels_path.exists() {
        let content = std::fs::read_to_string(&labels_path).unwrap_or_default();
        let parsed = parse_simple_toml(&content);
        labels_cfg.pii = parsed.get("pii").cloned().unwrap_or_default();
        labels_cfg.credentials = parsed.get("credentials").cloned().unwrap_or_default();
        labels_cfg.trust_zones = parsed.get("trust_zones").cloned().unwrap_or_default();
    }

    let mut total_inserted = 0;

    if let Err(e) = db.conn().execute_batch("BEGIN IMMEDIATE TRANSACTION") {
        return ToolCallResult::err(format!("Failed to begin transaction: {}", e));
    }

    if let Err(e) = db.conn().execute("DELETE FROM symbol_tags WHERE source = 'config'", []) {
        let _ = db.conn().execute_batch("ROLLBACK");
        return ToolCallResult::err(format!("Failed to clear old tags: {}", e));
    }

    // Helper closure to process a map
    let mut process_map = |map: &HashMap<String, Vec<String>>, tag_type: &str| -> Result<(), rusqlite::Error> {
        for (tag_value, patterns) in map {
            for pattern in patterns {
                let is_like = pattern.contains('*');
                let query_str = if is_like {
                    "INSERT OR IGNORE INTO symbol_tags (symbol_name, tag_type, tag_value, source) \
                     SELECT name, ?, ?, 'config' FROM symbols WHERE name LIKE ?".to_string()
                } else {
                    "INSERT OR IGNORE INTO symbol_tags (symbol_name, tag_type, tag_value, source) \
                     SELECT name, ?, ?, 'config' FROM symbols WHERE name = ?".to_string()
                };
                let search_param = pattern.replace('*', "%");

                let mut stmt = db.conn().prepare(&query_str)?;
                let rows = stmt.execute([tag_type, tag_value, &search_param]).unwrap_or(0);
                total_inserted += rows;
            }
        }
        Ok(())
    };

    // Apply security configs
    if let Err(e) = process_map(&security_cfg.sources, "source") {
        return ToolCallResult::err(format!("DB error processing sources: {}", e));
    }
    if let Err(e) = process_map(&security_cfg.sinks, "sink") {
        return ToolCallResult::err(format!("DB error processing sinks: {}", e));
    }
    if let Err(e) = process_map(&security_cfg.sanitizers, "sanitizer") {
        return ToolCallResult::err(format!("DB error processing sanitizers: {}", e));
    }

    // Apply label configs
    if let Err(e) = process_map(&labels_cfg.pii, "pii") {
        return ToolCallResult::err(format!("DB error processing pii: {}", e));
    }
    if let Err(e) = process_map(&labels_cfg.credentials, "credentials") {
        return ToolCallResult::err(format!("DB error processing credentials: {}", e));
    }
    if let Err(e) = process_map(&labels_cfg.trust_zones, "trust_zone") {
        return ToolCallResult::err(format!("DB error processing trust_zones: {}", e));
    }

    if let Err(e) = db.conn().execute_batch("COMMIT") {
        return ToolCallResult::err(format!("Failed to commit transaction: {}", e));
    }

    let msg = format!(
        "Successfully applied configuration overlays.\n\
         - Inserted {} tags into `symbol_tags`.\n\n\
         > ⚠️ **Limitation:** Pattern matching is based on symbol NAMES in the call graph.\n\
         > Macro invocations (sqlx::query!, format!, etc.) are NOT indexed as symbols.\n\
         > Use function wrapper patterns in security.toml instead.",
        total_inserted
    );

    ToolCallResult::ok(&msg)
}

/// Zero-dependency parser for TOML flat sections with string arrays.
/// Handles `[section]` and `key = ["val1", "val2::with::namespace",]`
fn parse_simple_toml(content: &str) -> HashMap<String, HashMap<String, Vec<String>>> {
    let mut result: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len()-1].to_string();
            result.entry(current_section.clone()).or_default();
            continue;
        }

        if let Some(eq_idx) = line.find('=') {
            let key = line[..eq_idx].trim().to_string();
            let val_part = line[eq_idx+1..].trim();
            if val_part.starts_with('[') {
                let inner = val_part.trim_start_matches('[').trim_end_matches(']');
                let mut vals = Vec::new();
                let mut in_quotes = false;
                let mut current_val = String::new();
                for c in inner.chars() {
                    if c == '"' {
                        in_quotes = !in_quotes;
                        if !in_quotes {
                            vals.push(current_val.clone());
                            current_val.clear();
                        }
                    } else if in_quotes {
                        current_val.push(c);
                    }
                }
                result.entry(current_section.clone()).or_default().insert(key, vals);
            }
        }
    }
    result
}
