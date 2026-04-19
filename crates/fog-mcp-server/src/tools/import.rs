//! fog-mcp-server/src/tools/import.rs
//!
//! fog_import - Tool #14: Import legacy knowledge (Layers 2/3/4) into fog-context.
//!
//! Sources:
//!   - ByteRover (.brv/context-tree/)  → Domains (Layer 2) + Constraints (Layer 3)
//!   - GitNexus  (.gitnexus/*.db)      → Domains + Constraints + Decisions (Layers 2/3/4)
//!
//! Intentionally imports ONLY Layers 2/3/4. Layer 1 (symbols/files/edges) is
//! deliberately excluded because indexer algorithms differ between systems -
//! merging raw symbol rows would cause primary-key conflicts and duplicate noise.
//!
//! PATTERN_DECISION: Level 2 (Composition of pure extractors)

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::protocol::{ToolCallResult, ToolDef};

// ---------------------------------------------------------------------------
// Tool registration
// ---------------------------------------------------------------------------

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_import",
        description: "Import legacy knowledge (Domains/Constraints/Decisions - Layers 2/3/4) \
                      from ByteRover (.brv/) or GitNexus (.gitnexus/) into fog-context. \
                      Layer 1 (symbols) is intentionally excluded to avoid indexer conflicts. \
                      Use source='auto' to detect both. Supports dry_run preview.",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "enum": ["auto", "brv", "gitnexus"],
                    "description": "'auto' detects .brv/ and/or .gitnexus/ automatically. 'brv' or 'gitnexus' for explicit source.",
                    "default": "auto"
                },
                "path": {
                    "type": "string",
                    "description": "Override path to .brv/ or .gitnexus/ directory. Defaults to project root."
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Preview what would be imported without writing to DB. Default: false.",
                    "default": false
                }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &fog_memory::MemoryDb, project_root: &Path) -> ToolCallResult {
    let source  = args["source"].as_str().unwrap_or("auto");
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let base    = args["path"].as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| project_root.to_path_buf());

    let mut report = ImportReport::default();

    // ── Detect what's available ──
    let brv_path  = base.join(".brv").join("context-tree");
    let gnx_paths = find_gitnexus_db(&base);

    match source {
        "brv" => {
            if !brv_path.exists() {
                return ToolCallResult::err(format!(
                    "No .brv/context-tree/ found at {}", base.display()
                ));
            }
            import_brv(&brv_path, db, dry_run, &mut report);
        }
        "gitnexus" => {
            if gnx_paths.is_empty() {
                return ToolCallResult::err(format!(
                    "No GitNexus DB found at {} or ~/.gitnexus/", base.display()
                ));
            }
            for gnx in &gnx_paths {
                import_gitnexus(gnx, db, dry_run, &mut report);
            }
        }
        _ => {
            // auto: try both
            if brv_path.exists() {
                import_brv(&brv_path, db, dry_run, &mut report);
            }
            for gnx in &gnx_paths {
                import_gitnexus(gnx, db, dry_run, &mut report);
            }
            if !brv_path.exists() && gnx_paths.is_empty() {
                return ToolCallResult::err(
                    String::from("No .brv/ or .gitnexus/ found. Use fog_import with explicit path.")
                );
            }
        }
    }

    ToolCallResult::ok(report.render(dry_run))
}

// ---------------------------------------------------------------------------
// Import tracking
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ImportReport {
    brv_domains:     Vec<String>,
    brv_constraints: Vec<String>,
    gnx_domains:     usize,
    gnx_constraints: usize,
    gnx_decisions:   usize,
    errors:          Vec<String>,
}

impl ImportReport {
    fn render(&self, dry_run: bool) -> String {
        let prefix = if dry_run { "🔍 DRY RUN - " } else { "" };
        let action = if dry_run { "Would import" } else { "Imported" };
        let mut out = format!("# {}fog_import Results\n\n", prefix);

        if !self.brv_domains.is_empty() || !self.brv_constraints.is_empty() {
            out += "## ByteRover (.brv)\n";
            out += &format!("- **{action} {n} domains:** {list}\n",
                n = self.brv_domains.len(),
                list = self.brv_domains.join(", "));
            out += &format!("- **{action} {} constraints** from conventions/architecture\n",
                self.brv_constraints.len());
        }

        if self.gnx_domains + self.gnx_constraints + self.gnx_decisions > 0 {
            out += "\n## GitNexus (Layer 2/3/4 only - symbols excluded)\n";
            out += &format!("- **{action} {} domains**\n", self.gnx_domains);
            out += &format!("- **{action} {} constraints**\n", self.gnx_constraints);
            out += &format!("- **{action} {} decisions**\n", self.gnx_decisions);
        }

        if !self.errors.is_empty() {
            out += "\n## Warnings\n";
            for e in &self.errors { out += &format!("- {e}\n"); }
        }

        if !dry_run && (self.brv_domains.len() + self.gnx_domains) > 0 {
            out += "\n**Next:** Run `fog_domains` to verify, then `fog_lookup` to see bulk-mapped symbols.";
        }

        out
    }
}

// ---------------------------------------------------------------------------
// ByteRover extractor
// ---------------------------------------------------------------------------

fn import_brv(ctx_tree: &Path, db: &fog_memory::MemoryDb, dry_run: bool, report: &mut ImportReport) {
    use fog_memory::write::DefineDomainArgs;

    // ── Layer 2: Domains from root _index.md frontmatter ──
    let root_index = ctx_tree.join("_index.md");
    if let Ok(content) = fs::read_to_string(&root_index) {
        let domains = extract_brv_domains(&content);
        report.brv_domains = domains.iter().map(|d| d.name.clone()).collect();
        if !dry_run {
            for d in &domains {
                let _ = db.define_domain(DefineDomainArgs {
                    name: d.name.clone(),
                    keywords: Some(d.keywords.clone()),
                    symbols: None,
                    constraints: None,
                });
                // Auto bulk-map: FTS keyword → domain_symbols via direct SQL
                // We use direct connection since fog-memory doesn't expose this method
                let fog_db_path = db.db_path().to_path_buf();
                if let Ok(conn) = Connection::open(&fog_db_path) {
                    conn.query_row(
                        "SELECT id FROM domains WHERE name = ?1",
                        rusqlite::params![d.name],
                        |r| r.get::<_, i64>(0)
                    ).ok().map(|domain_id| {
                        for kw in &d.keywords {
                            let _ = conn.execute(
                                "INSERT OR IGNORE INTO domain_symbols (domain_id, symbol_name)
                                 SELECT ?1, name FROM symbols
                                 WHERE name LIKE '%' || ?2 || '%'
                                    OR name_tokens LIKE '%' || ?2 || '%'",
                                rusqlite::params![domain_id, kw],
                            );
                        }
                    });
                }
            }
        }
    } else {
        report.errors.push(format!("Could not read {}", root_index.display()));
    }

    // ── Layer 3: Constraints from subdirectory _index.md files ──
    let constraint_dirs = ["conventions", "architecture", "standards", "rules"];
    for dir in &constraint_dirs {
        let idx = ctx_tree.join(dir).join("_index.md");
        if let Ok(content) = fs::read_to_string(&idx) {
            let constraints = extract_brv_constraints(&content, dir);
            report.brv_constraints.extend(constraints.iter().map(|c| c.code.clone()));
            if !dry_run {
                for c in &constraints {
                    let _ = db.insert_constraint(&c.code, "WARNING", &c.statement);
                }
            }
        }
    }
}

struct DomainDraft { name: String, keywords: Vec<String> }
struct ConstraintDraft { code: String, statement: String }

/// Extract domains from BRV root _index.md YAML frontmatter.
/// covers: [architecture/_index.md, features/_index.md] → ["Architecture", "Features"]
fn extract_brv_domains(content: &str) -> Vec<DomainDraft> {
    let mut domains = Vec::new();
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut fence_count = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            fence_count += 1;
            in_frontmatter = fence_count == 1;
            if fence_count == 2 { frontmatter_done = true; in_frontmatter = false; }
            continue;
        }
        if !in_frontmatter || frontmatter_done { continue; }

        // covers: [architecture/_index.md, features/_index.md]
        if let Some(rest) = trimmed.strip_prefix("covers:") {
            let inner = rest.trim().trim_start_matches('[').trim_end_matches(']');
            for part in inner.split(',') {
                let part = part.trim().trim_matches('"').trim_matches('\'');
                // Take directory name before '/'
                let dir_name = part.split('/').next().unwrap_or(part).trim();
                if dir_name.is_empty() || dir_name.starts_with('_') { continue; }
                let name = dir_name.replace('_', " ")
                    .split_whitespace()
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().to_string() + c.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let keywords = dir_name.split('_').map(|s| s.to_lowercase()).collect();
                domains.push(DomainDraft { name, keywords });
            }
        }
    }
    domains
}

/// Extract constraints from BRV subdir _index.md bullet points.
/// Pattern: * **Context Protocol:** Enforces single-phase...
fn extract_brv_constraints(content: &str, source_dir: &str) -> Vec<ConstraintDraft> {
    let mut constraints = Vec::new();
    // Regex-style: lines matching "* **Name:**..." or "- **Name:**..."
    let re_prefix = |line: &str| -> Option<(String, String)> {
        let line = line.trim();
        let line = line.strip_prefix("* ").or_else(|| line.strip_prefix("- "))?;
        let line = line.strip_prefix("**")?;
        let (name_part, rest) = line.split_once("**")?;
        let name = name_part.trim_end_matches(':').trim().to_string();
        let statement = rest.trim_start_matches(':').trim().to_string();
        if name.is_empty() || statement.is_empty() { return None; }
        // Filter noise bullets that are navigation/linking, not constraints
        let lower = name.to_lowercase();
        let is_noise = lower.starts_with("reference") || lower.starts_with("see also")
            || lower.starts_with("note") || lower.starts_with("example")
            || lower.starts_with("related") || lower.starts_with("source");
        if is_noise { return None; }
        Some((name, statement))
    };

    for line in content.lines() {
        if let Some((name, statement)) = re_prefix(line) {
            // code = "SOURCE_DIR.NAME" style, uppercase snake
            let code = format!("{}.{}",
                source_dir.to_uppercase().replace('-', "_"),
                name.to_uppercase().replace(' ', "_").replace('-', "_")
            );
            constraints.push(ConstraintDraft { code, statement });
        }
    }
    constraints
}

// ---------------------------------------------------------------------------
// GitNexus SQL pump (Layers 2/3/4 only - no symbols)
// ---------------------------------------------------------------------------

fn find_gitnexus_db(project_root: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![
        project_root.join(".gitnexus").join("context.db"),
        project_root.join(".gitnexus").join("global.db"),
    ];
    // Also check home dir global
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(&home).join(".gitnexus").join("global.db"));
        candidates.push(PathBuf::from(&home).join(".gitnexus").join("context.db"));
    }
    candidates.into_iter().filter(|p| p.exists()).collect()
}

fn import_gitnexus(gnx_path: &Path, db: &fog_memory::MemoryDb, dry_run: bool, report: &mut ImportReport) {
    let fog_db_path = db.db_path().to_path_buf();

    // Open a direct connection to fog-context DB for ATTACH
    let conn = match Connection::open(&fog_db_path) {
        Ok(c) => c,
        Err(e) => { report.errors.push(format!("Cannot open fog DB: {e}")); return; }
    };

    let gnx_str = gnx_path.to_string_lossy();

    // Attach legacy DB
    if let Err(e) = conn.execute_batch(&format!(
        "ATTACH DATABASE '{gnx_str}' AS legacy;"
    )) {
        report.errors.push(format!("Cannot attach {gnx_str}: {e}"));
        return;
    }

    // ── Probe schema: detect if legacy has our expected tables ──
    let has_domains: bool = conn.query_row(
        "SELECT COUNT(*) FROM legacy.sqlite_master WHERE type='table' AND name='domains'",
        [], |r| r.get(0)
    ).unwrap_or(0) > 0;
    let has_constraints: bool = conn.query_row(
        "SELECT COUNT(*) FROM legacy.sqlite_master WHERE type='table' AND name='constraints'",
        [], |r| r.get(0)
    ).unwrap_or(0) > 0;
    let has_decisions: bool = conn.query_row(
        "SELECT COUNT(*) FROM legacy.sqlite_master WHERE type='table' AND name='decisions'",
        [], |r| r.get(0)
    ).unwrap_or(0) > 0;

    if !dry_run {
        // Layer 2: Domains
        if has_domains {
            let pumped: i64 = conn.execute_batch(
                "INSERT OR IGNORE INTO main.domains (name, keywords, auto)
                 SELECT name, keywords, COALESCE(auto, 0) FROM legacy.domains
                 WHERE name IS NOT NULL AND name != '';"
            ).map(|_| 0).unwrap_or(0);
            // Count what was actually inserted
            report.gnx_domains = conn.query_row(
                "SELECT changes()", [], |r| r.get::<_, i64>(0)
            ).unwrap_or(pumped) as usize;
        }

        // Layer 3: Constraints
        if has_constraints {
            conn.execute_batch(
                "INSERT OR IGNORE INTO main.constraints (code, statement, severity, source_file)
                 SELECT code, statement, COALESCE(severity,'WARNING'), source_file
                 FROM legacy.constraints
                 WHERE code IS NOT NULL AND statement IS NOT NULL;"
            ).ok();
            report.gnx_constraints = conn.query_row(
                "SELECT changes()", [], |r| r.get::<_, i64>(0)
            ).unwrap_or(0) as usize;
        }

        // Layer 4: Decisions (causality log - always safe to merge)
        if has_decisions {
            conn.execute_batch(
                "INSERT OR IGNORE INTO main.decisions (domain, functions, reason, revert_risk, status, created_at)
                 SELECT domain, functions, reason, COALESCE(revert_risk,'LOW'),
                        COALESCE(status,'active'), created_at
                 FROM legacy.decisions
                 WHERE reason IS NOT NULL AND reason != '';"
            ).ok();
            report.gnx_decisions = conn.query_row(
                "SELECT changes()", [], |r| r.get::<_, i64>(0)
            ).unwrap_or(0) as usize;
        }
    } else {
        // Dry run: just count rows
        if has_domains {
            report.gnx_domains = conn.query_row(
                "SELECT COUNT(*) FROM legacy.domains WHERE name NOT IN (SELECT name FROM main.domains)",
                [], |r| r.get::<_, i64>(0)
            ).unwrap_or(0) as usize;
        }
        if has_constraints {
            report.gnx_constraints = conn.query_row(
                "SELECT COUNT(*) FROM legacy.constraints WHERE code NOT IN (SELECT code FROM main.constraints)",
                [], |r| r.get::<_, i64>(0)
            ).unwrap_or(0) as usize;
        }
        if has_decisions {
            report.gnx_decisions = conn.query_row(
                "SELECT COUNT(*) FROM legacy.decisions WHERE reason IS NOT NULL",
                [], |r| r.get::<_, i64>(0)
            ).unwrap_or(0) as usize;
        }
    }

    let _ = conn.execute_batch("DETACH DATABASE legacy;");
}
