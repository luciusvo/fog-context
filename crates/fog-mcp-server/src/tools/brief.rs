//! fog_brief - server/index status overview.
//!
//! Replaces: health
//! CALL FIRST when starting a task to verify the index is fresh.
//!
//! F3: Now shows active project identity (name, path, fog_id) so agents
//! can instantly verify they are connected to the correct project.

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use std::path::Path;
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;
use crate::registry::ProjectConfig;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_brief",
        description: "Check fog-context index status: symbol count, file count, last indexed time, \
            schema version, and knowledge layer population (domains, decisions, constraints). \
            MANDATORY FIRST STEP - if symbols = 0, call fog_scan before anything else.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Project path or name. Omit to use current project."
                }
            }
        }),
    }
}

pub fn handle(
    _args: &Value,
    db: &MemoryDb,
    registry: &crate::registry::Registry,
    project_root: &Path,
) -> ToolCallResult {
    match db.knowledge_score() {
        Ok(score) => {
            let status = if score.total_symbols == 0 {
                "⚠️  NOT INDEXED - call fog_scan first"
            } else if score.layer_score >= 75 {
                "✅ Healthy"
            } else if score.layer_score >= 40 {
                "🟡 Partial - see knowledge gaps below"
            } else {
                "🔴 Low knowledge quality"
            };

            // C2+C3: Version check
            let binary_version = env!("CARGO_PKG_VERSION");
            let cfg = ProjectConfig::load(project_root);
            let indexed_version = cfg.indexer_version.clone();
            let version_banner = match &indexed_version {
                None => format!(
                    "\n> ⚠️ **Index not yet created** — run `fog_scan` to build the knowledge graph.\n"
                ),
                Some(v) if v != binary_version => format!(
                    "\n> 🆕 **New binary detected!** Binary: `v{binary_version}` | Index built by: `v{v}`\n\
                     > Run `fog_scan({{ \"project\": \"{}\" }})` to refresh the index with the new version.\n",
                    project_root.display()
                ),
                _ => String::new(),
            };

            // F3: Resolve project identity from config.toml
            // Use ensure_project_id (not read_project_id) so fog_id is always set
            let fog_id = crate::registry::ensure_project_id(&project_root.to_string_lossy());

            let project_name = ProjectConfig::load(project_root)
                .name
                .or_else(|| {
                    project_root.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "unknown".to_string());

            // DB file size
            let db_size = {
                let db_path = project_root.join(".fog-context").join("context.db");
                if let Ok(meta) = std::fs::metadata(&db_path) {
                    let kb = meta.len() / 1024;
                    if kb > 1024 { format!("{:.1} MB", kb as f64 / 1024.0) }
                    else { format!("{} KB", kb) }
                } else {
                    "not found".to_string()
                }
            };

            let mut lines = vec![
                format!("# fog-context v{binary_version} — Status{version_banner}"),
                // fog_id FIRST — agents must capture this for all subsequent calls
                "## 🔑 Project Identity".to_string(),
                format!("**fog_id:** `{fog_id}`  ← use this in all calls: `{{ \"project\": \"{fog_id}\" }}`"),
                format!("**Name:** {}", project_name),
                format!("**Path:** `{}`", project_root.display()),
                format!("**DB:** .fog-context/context.db ({})", db_size),
                format!("**Indexed by:** v{}", indexed_version.unwrap_or_else(|| "(not yet indexed)".to_string())),
                String::new(),
                "## 📊 Index Status".to_string(),
                format!("**State:** {status}"),
                format!("**Knowledge Score:** {}/100", score.layer_score),
                format!("**Symbols:** {} across {} files", score.total_symbols, score.total_files),
                format!("**Edges:** {}", score.total_edges),
                format!("**Domains (L2):** {}", score.total_domains),
                format!("**Constraints (L3):** {}", score.total_constraints),
                format!("**Decisions (L4):** {}", score.total_decisions),
                format!("**Projects in registry:** {}", registry.list().len()),
            ];

            // Sprint 3D: mandatory enforcement reminder for empty layers
            if score.total_symbols > 0 {
                let mut gaps: Vec<String> = Vec::new();

                if score.total_domains == 0 {
                    gaps.push(
                        "  → **Layer 2 empty** - group symbols into domains:\n\
                         ```\n\
                         fog_assign({ \"domain\": \"YourFeature\", \"symbols\": [\"fn_a\", \"fn_b\"] })\n\
                         ```".to_string()
                    );
                }
                if score.total_constraints == 0 {
                    gaps.push(
                        "  → **Layer 3 empty** - load architecture rules:\n\
                         ```\n\
                         fog_constraints({})       ← scan ADR files\n\
                         fog_constraints({ \"init\": true })   ← or bootstrap template\n\
                         ```".to_string()
                    );
                }
                if score.total_decisions == 0 {
                    gaps.push(
                        "  → **⚠️  Layer 4 EMPTY** - no decisions recorded. MANDATORY after every change:\n\
                         ```\n\
                         fog_decisions({ \"functions\": [\"changed_fn\"], \"reason\": \"WHY\", \"revert_risk\": \"LOW\" })\n\
                         ```".to_string()
                    );
                }

                if !gaps.is_empty() {
                    lines.push(format!("\n## 🔴 Knowledge Gaps (Action Required)\n{}", gaps.join("\n\n")));
                }
            }

            // #15: Grammar warnings from last scan (stored in registry)
            let path_str = project_root.to_string_lossy();
            if let Some(entry) = registry.find(&path_str) {
                if !entry.grammar_warnings.is_empty() {
                    lines.push("\n## ⚠️ Grammar Warnings (from last fog_scan)".to_string());
                    lines.push("> Some languages may have incomplete symbol graphs:".to_string());
                    for w in &entry.grammar_warnings {
                        lines.push(format!("> - `{w}`"));
                    }
                    lines.push("> Run `fog_scan({})` after updating fog-context to fix.".to_string());
                }
            }

            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_brief error: {e}")),
    }
}
