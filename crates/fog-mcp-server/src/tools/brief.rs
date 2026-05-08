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
    session_stats: Option<(u64, u64)>,
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

            // Fix 1: Eagerly register the project path in the global registry.
            // fog_brief may be the first call on a new project — if we don't register here,
            // the fog_id returned in this response cannot be used to call fog_scan.
            // Chicken-and-egg fix: register with 0 symbols (fog_scan will update the count later).
            {
                let mut reg = crate::registry::Registry::load();
                let path_str = project_root.to_string_lossy().into_owned();
                if reg.find(&fog_id).is_none() && reg.find(&path_str).is_none() {
                    let reg_name = project_root.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "unknown".into());
                    reg.register(reg_name, path_str);
                }
            }

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

            // Calculate L2 Coverage & Domain Drift
            let mapped_count: i64 = db.conn().query_row("SELECT COUNT(DISTINCT symbol_name) FROM domain_symbols", [], |r| r.get(0)).unwrap_or(0);
            let total_symbols: i64 = db.conn().query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0)).unwrap_or(0);
            
            let mut l2_warning = String::new();
            if total_symbols > 0 {
                let pct = (mapped_count as f64 / total_symbols as f64 * 100.0) as u8;
                if pct < 30 {
                    l2_warning = format!("  ⚠️ L2 Domain coverage: {pct}% ({mapped_count}/{total_symbols})");
                }
            }

            let orphan_mappings: i64 = db.conn().query_row(
                "SELECT COUNT(*) FROM domain_symbols ds LEFT JOIN symbols s ON s.name = ds.symbol_name WHERE s.id IS NULL AND ds.symbol_name IS NOT NULL", 
                [], 
                |row| row.get(0)
            ).unwrap_or(0);
            let mut drift_warning = String::new();
            if orphan_mappings > 0 {
                drift_warning = format!("  ⚠️ Domain drift detected: {orphan_mappings} mapped symbols no longer exist in code.");
            }

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
                "## 📊 Context Graph Maturity".to_string(),
                format!("**State:** {status}"),
                "**Layers Populated:**".to_string(),
                if score.total_symbols > 0 {
                    format!("  [■■■■■] L1 Physical:  {} Symbols, {} Edges", score.total_symbols, score.total_edges)
                } else {
                    "  [□□□□□] L1 Physical:  0 Symbols (Not indexed)".to_string()
                },
                if score.total_domains > 0 {
                    format!("  [■■■■■] L2 Business:  {} Domains defined", score.total_domains)
                } else {
                    "  [□□□□□] L2 Business:  0 Domains defined".to_string()
                },
                if score.total_constraints > 0 {
                    format!("  [■■■■■] L3 Rules:     {} Constraints (ADRs)", score.total_constraints)
                } else {
                    "  [□□□□□] L3 Rules:     0 Constraints (ADRs)".to_string()
                },
                if score.total_decisions > 0 {
                    format!("  [■■■■■] L4 Causality: {} Decision Records", score.total_decisions)
                } else {
                    "  [□□□□□] L4 Causality: 0 Decision Records".to_string()
                },
            ];

            if !db.verify_json1() {
                lines.push("  L4 Coverage: ❓ (requires JSON1 extension)".to_string());
            } else {
                let coverage_query = "
                    WITH important_symbols AS (
                        SELECT s.name
                        FROM symbols s
                        JOIN edges e ON e.target_id = s.id AND e.kind = 'CALLS'
                        GROUP BY s.id
                        HAVING COUNT(DISTINCT e.source_id) >= 3
                    )
                    SELECT
                        COUNT(DISTINCT i.name) as total_important,
                        COUNT(DISTINCT CASE WHEN EXISTS (
                            SELECT 1 FROM decisions d
                            WHERE EXISTS (SELECT 1 FROM json_each(d.functions) WHERE value = i.name)
                        ) THEN i.name END) as covered
                    FROM important_symbols i";
                
                if let Ok((total_important, covered)) = db.conn().query_row(coverage_query, [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
                    if total_important > 0 {
                        let pct = (covered as f64 / total_important as f64 * 100.0) as u8;
                        let warn = if pct < 50 { " ⚠️" } else { "" };
                        lines.push(format!("  L4 Coverage: {}% ({}/{}) high-impact functions documented{}", pct, covered, total_important, warn));
                    } else {
                        lines.push("  L4 Coverage: 100% (No high-impact functions detected)".to_string());
                    }
                }
            }

            if !l2_warning.is_empty() {
                lines.push(l2_warning);
            }
            if !drift_warning.is_empty() {
                lines.push(drift_warning);
            }
            lines.push(String::new());

            let tag_count: i64 = match db.conn().query_row("SELECT COUNT(*) FROM symbol_tags", [], |row| row.get(0)) {
                Ok(c) => c,
                Err(rusqlite::Error::SqliteFailure(_e, Some(msg))) if msg.contains("no such table") => 0,
                Err(e) => return ToolCallResult::err(format!("Database error querying symbol_tags: {e}")),
            };
            lines.push("## 🛡️ Security Overlay".to_string());
            if tag_count > 0 {
                lines.push(format!("- **Tags applied:** {tag_count}"));
            } else {
                lines.push("- Status: ❌ No tags. Run `fog_bootstrap` then `fog_overlay`.".to_string());
            }
            lines.push(String::new());

            #[cfg(feature = "embedding")]
            {
                lines.push("## 🔍 Semantic Search (Embedded Variant)".to_string());
                if crate::semantic::get_model().is_some() {
                    lines.push("Status: ✅ Active".to_string());
                } else {
                    lines.push("> [!WARNING]".to_string());
                    lines.push("> **Model not found**".to_string());
                    lines.push("> Place model and tokenizer at: ~/.fog/models/".to_string());
                    lines.push("> ```bash".to_string());
                    lines.push("> curl -L https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model_quantized.onnx -o ~/.fog/models/all-MiniLM-L6-v2-q8.onnx".to_string());
                    lines.push("> curl -L https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json -o ~/.fog/models/tokenizer.json".to_string());
                    lines.push("> ```".to_string());
                    lines.push("> Reference: https://github.com/luciusvo/fog-context#semantic-setup".to_string());
                }
            }
            #[cfg(not(feature = "embedding"))]
            {
                lines.push("## 🔍 Semantic Search (Normal Variant)".to_string());
                lines.push("Status: ❌ Disabled in this build".to_string());
            }

            lines.push(String::new());
            lines.push(format!("**Projects in registry:** {}", registry.list().len()));

            // Sprint 3D: mandatory enforcement reminder for empty layers
            if score.total_symbols > 0 {
                if score.total_domains == 0 || score.total_constraints == 0 {
                    lines.push("---".to_string());
                    lines.push(r#"
## 🔴 REQUIRED ACTIONS
- L2 (Business Domains): fog_assign({ "domain": "...", "symbols": [...] })
- L3 (Constraints): fog_constraints({})

Run before proceeding with any code analysis."#.to_string());
                }

                lines.push(r#"
## 📋 Session Protocol
- **BEFORE editing:** fog_impact({ "target": "function_name" })
- **AFTER editing:** fog_decisions({ "functions": [...], "reason": "WHY" })"#.to_string());
            }

            if let Some((project_calls, total_calls)) = session_stats {
                lines.push("\n## 📈 Session Stats".to_string());
                lines.push(format!("- Tools invoked (this project): **{}**", project_calls));
                lines.push(format!("- Tools invoked (global pool):  **{}**", total_calls));
            }

            ToolCallResult::ok(lines.join("\n"))
        }
        Err(e) => ToolCallResult::err(format!("fog_brief error: {e}")),
    }
}
