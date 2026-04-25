//! fog-mcp-server/src/indexer/ingest.rs
//!
//! Two-pass DB ingestion using Tree-sitter 0.24 (StreamingIterator API).
//!
//! Pass 1 - Parse each file, insert symbols + intra-file edges.
//!           Queue unresolved cross-file calls for Pass 2.
//! Pass 2 - Resolve deferred calls against the complete symbol index.
//!           Insert cross-file CALLS edges.
//!
//! PATTERN_DECISION: Level 2 (Composition)
//! Isolation: this module uses rusqlite directly via a Connection ref
//! obtained from the DB file path - fog-memory::MemoryDb::conn() is crate-private.
//! We open our own connection for the indexer to avoid the borrow checker
//! complexity of mixing rusqlite + tree-sitter lifetimes.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use super::langs::config_for;
use super::walker::ScannedFile;
use super::IndexStats;

// Error type surfaced when a language query fails to compile.
// Distinct from IO errors so fog_scan can report them clearly.
#[derive(Debug)]
pub(crate) struct QueryCompileError {
    pub lang: String,
    pub detail: String,
}

impl std::fmt::Display for QueryCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Query compile error for '{}': {}", self.lang, self.detail)
    }
}
impl std::error::Error for QueryCompileError {}

// ---------------------------------------------------------------------------
// Stdlib call noise filter (mirrors TS indexer)
// ---------------------------------------------------------------------------

const STDLIB_NOISE: &[&str] = &[
    "new", "clone", "fmt", "from", "into", "default", "drop", "len", "is_empty",
    "iter", "map", "filter", "unwrap", "expect", "ok", "err", "some", "none",
    "push", "pop", "get", "insert", "remove", "to_string", "as_str",
    "send", "recv", "lock", "read", "write", "flush", "close", "open",
    "parse", "format", "print", "println", "then", "catch", "resolve",
    "append", "extend", "update", "keys", "values", "items", "copy",
];

fn is_noise(name: &str) -> bool {
    STDLIB_NOISE.contains(&name) || name.len() < 3
}

// ---------------------------------------------------------------------------
// Deferred cross-file edge
// ---------------------------------------------------------------------------

/// A deferred cross-file edge (resolved in Pass 2).
/// edge_kind distinguishes CALLS from DI_INJECT, IMPLEMENTS, DYNAMIC_IMPORT, etc.
pub(super) struct Deferred {
    pub(super) source_id: i64,
    pub(super) target_name: String,
    pub(super) edge_kind: &'static str,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the two-pass indexer. Opens a separate rusqlite connection to the DB
/// (fog-memory's MemoryDb::conn() is crate-private; we use the DB path).
pub fn run_two_pass(
    project_root: &Path,
    db: &fog_memory::MemoryDb,
    scanned: &[ScannedFile],
    full: bool,
) -> Result<IndexStats, Box<dyn std::error::Error>> {
    // Open a direct connection to the same DB file
    let db_path = db.db_path().to_path_buf();
    let conn = Connection::open(&db_path)?;
    // CRITICAL: set busy_timeout FIRST on this new connection.
    // PRAGMA journal_mode = WAL needs an exclusive lock to switch modes.
    // Without busy_timeout set first, that switch fails immediately with
    // "database is locked" if the MCP server's connection is open.
    conn.execute_batch("PRAGMA busy_timeout = 30000;")?; // 30s retry window
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")?;

    run_two_pass_conn(project_root, &conn, scanned, full)
}

fn run_two_pass_conn(
    project_root: &Path,
    conn: &Connection,
    scanned: &[ScannedFile],
    full: bool,
) -> Result<IndexStats, Box<dyn std::error::Error>> {
    let mut stats = IndexStats::default();
    stats.files_total = scanned.len();

    // Write hint template if first run (idempotent)
    super::hints::write_template_if_missing(project_root);

    // ── Load existing checksums for incremental mode ──
    let mut existing: HashMap<String, (i64, String)> = HashMap::new();
    if !full {
        let mut stmt = conn.prepare("SELECT id, path, content_hash FROM files")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?;
        for row in rows.flatten() {
            existing.insert(row.1, (row.0, row.2));
        }
    }

    let current_paths: HashSet<&str> = scanned.iter().map(|f| f.path.as_str()).collect();
    let to_index: Vec<&ScannedFile> = scanned.iter()
        .filter(|f| match existing.get(&f.path) {
            Some((_, ck)) => ck != &f.checksum,
            None => true,
        })
        .collect();

    let deleted: Vec<String> = existing.keys()
        .filter(|p| !current_paths.contains(p.as_str()))
        .cloned().collect();
    stats.files_deleted = deleted.len();

    // ── Pass 1: ingest files in 500-file batches (#13: large-repo checkpoint) ──
    let mut all_deferred: Vec<Deferred> = Vec::new();

    // Delete removed files first (CASCADE cleans symbols/edges)
    for path in &deleted {
        conn.execute("DELETE FROM files WHERE path = ?1", rusqlite::params![path])?;
    }

    let sql_upsert_file =
        "INSERT INTO files (path, lang, size_bytes, line_count, content_hash, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(path) DO UPDATE SET
           lang=excluded.lang, size_bytes=excluded.size_bytes,
           line_count=excluded.line_count, content_hash=excluded.content_hash,
           indexed_at=datetime('now')";

    const BATCH_SIZE: usize = 500;
    for chunk in to_index.chunks(BATCH_SIZE) {
        conn.execute_batch("BEGIN")?;
        for file in chunk {
            let full_path = project_root.join(&file.path);
            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            conn.execute(sql_upsert_file, rusqlite::params![
                file.path, file.lang,
                file.size_bytes as i64, file.line_count as i64,
                file.checksum,
            ])?;
            let file_id: i64 = conn.query_row(
                "SELECT id FROM files WHERE path = ?1",
                rusqlite::params![file.path],
                |r| r.get(0),
            )?;
            conn.execute("DELETE FROM symbols WHERE file_id = ?1", rusqlite::params![file_id])?;

            if let Some(cfg) = config_for(file.lang) {
                match parse_file(conn, file_id, &content, &cfg, &mut stats) {
                    Ok(deferred) => all_deferred.extend(deferred),
                    Err(e) => {
                        let msg = format!("{}: {}", cfg.name, e);
                        if !stats.query_errors.contains(&msg) {
                            stats.query_errors.push(msg.clone());
                        }
                        log_global_parser_error(cfg.name, &e.to_string());
                    }
                }
                // Approach 2: inject macro_expansion hints from .fog-context/hints/{lang}.json
                let lang_hints = super::hints::load(project_root, cfg.name);
                for (pattern, resolves_to) in &lang_hints.macro_expansions {
                    if let Ok(source_id) = conn.query_row(
                        "SELECT id FROM symbols WHERE file_id = ?1 ORDER BY start_line LIMIT 1",
                        rusqlite::params![file_id],
                        |r| r.get::<_, i64>(0),
                    ) {
                        all_deferred.push(Deferred {
                            source_id,
                            target_name: resolves_to.clone(),
                            edge_kind: "MACRO_EXPAND",
                        });
                        all_deferred.push(Deferred {
                            source_id,
                            target_name: pattern.clone(),
                            edge_kind: "MACRO_EXPAND",
                        });
                    }
                }
            }
            stats.files_indexed += 1;
        }
        conn.execute_batch("COMMIT")?;
        eprintln!("[fog] Pass 1 checkpoint: {}/{} files", stats.files_indexed, stats.files_total);
    }

    eprintln!("[fog] 3/5 🔎 Rebuilding full-text search index (this blocks for a moment)...");

    // Rebuild FTS after all batches complete
    conn.execute_batch("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild');")?;

    // ── Pass 2: cross-file resolution ──
    eprintln!("[fog] 4/5 🔗 Resolving cross-file references ({} items)...", all_deferred.len());
    if !all_deferred.is_empty() {
        let mut global: HashMap<String, Vec<i64>> = HashMap::new();
        let mut stmt = conn.prepare("SELECT id, name FROM symbols")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows.flatten() {
            global.entry(row.1).or_default().push(row.0);
        }

        let mut seen: HashSet<String> = HashSet::new();
        let total_deferred = all_deferred.len();
        for (i, d) in all_deferred.iter().enumerate() {
            let Some(targets) = global.get(&d.target_name) else { continue };
            let Some(&tid) = targets.first() else { continue };
            // Dedup key includes edge_kind to allow same symbol pair with different kinds
            let key = format!("{}:{}:{}", d.source_id, tid, d.edge_kind);
            if seen.insert(key) {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO edges (source_id, target_id, kind, confidence)
                     VALUES (?1, ?2, ?3, 'name_match')",
                    rusqlite::params![d.source_id, tid, d.edge_kind],
                );
                stats.edges_cross += 1;
            }
            if i > 0 && i % 5000 == 0 {
                eprintln!("[fog] Pass 2 checkpoint: {}/{} references resolved", i, total_deferred);
            }
        }
    }

    eprintln!("[fog] 5/5 💾 Flushing database to disk (WAL checkpoint)...");


    // ── Update meta + WAL checkpoint ──
    conn.execute(
        "INSERT INTO meta(key,value) VALUES('last_index',datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [],
    )?;
    // #G2-replacement: flush WAL so pool connections see latest data immediately
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");

    Ok(stats)
}

// ---------------------------------------------------------------------------
// Single-file parse + DB insert
// ---------------------------------------------------------------------------

fn parse_file(
    conn: &Connection,
    file_id: i64,
    content: &str,
    cfg: &super::langs::LangConfig,
    stats: &mut IndexStats,
) -> Result<Vec<Deferred>, Box<dyn std::error::Error>> {
    let mut parser = Parser::new();
    parser.set_language(&cfg.ts_language)?;
    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return Ok(vec![]),
    };
    let root = tree.root_node();
    let src = content.as_bytes();

    // ── Symbol definitions ──
    // Using new query schema: @name (identifier), @def (full definition node)
    // Pattern index maps to cfg.kinds[pattern_index]
    let def_q = match Query::new(&cfg.ts_language, cfg.def_query) {
        Ok(q) => q,
        Err(e) => {
            // Return a descriptive error - caller (run_two_pass_conn) collects
            // these into stats.query_errors[] so fog_scan can surface them.
            // NEVER silently Ok(vec![]) - that's the bug that produced 0 symbols.
            return Err(Box::new(QueryCompileError {
                lang: cfg.name.to_string(),
                detail: e.to_string(),
            }));
        }
    };

    // Capture indices for @name and @def
    let name_cap_idx = def_q.capture_names().iter().position(|n| *n == "name").map(|i| i as u32);
    let def_cap_idx  = def_q.capture_names().iter().position(|n| *n == "def").map(|i| i as u32);

    let mut cursor = QueryCursor::new();
    let mut local_ids: HashMap<String, i64> = HashMap::new();

    {
        let mut matches = cursor.matches(&def_q, root, src);
        while let Some(m) = matches.next() {
            // Kind comes from pattern_index → cfg.kinds
            let kind = cfg.kinds.get(m.pattern_index).copied().unwrap_or("unknown");

            let name_node = name_cap_idx.and_then(|idx| {
                m.captures.iter().find(|c| c.index == idx)
            }).map(|c| c.node);
            let def_node = def_cap_idx.and_then(|idx| {
                m.captures.iter().find(|c| c.index == idx)
            }).map(|c| c.node);

            let (Some(name_n), Some(def_n)) = (name_node, def_node) else { continue };

            let name = match name_n.utf8_text(src) {
                Ok(s) if !s.is_empty() => s.to_string(),
                _ => continue,
            };
            let start_line = (def_n.start_position().row + 1) as i64;
            let end_line   = (def_n.end_position().row + 1) as i64;
            let sig = def_n.utf8_text(src).ok()
                .and_then(|t| t.lines().next())
                .map(|l| l.trim().chars().take(120).collect::<String>());
            let doc = extract_doc(content, def_n.start_position().row);
            let tokens = tokenize_name(&name);

            conn.execute(
                "INSERT INTO symbols
                 (file_id, name, kind, start_line, end_line, signature, doc, name_tokens, centrality)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0.0)",
                rusqlite::params![file_id, name, kind, start_line, end_line, sig, doc, tokens],
            )?;
            local_ids.insert(name, conn.last_insert_rowid());
            stats.symbols_created += 1;
        }
    }

    // ── Call edges ──
    let call_q = match Query::new(&cfg.ts_language, cfg.call_query) {
        Ok(q) => q,
        Err(_) => return Ok(vec![]),
    };

    // @name capture in call queries
    let call_name_idx = call_q.capture_names().iter()
        .position(|n| *n == "name").map(|i| i as u32);

    let mut cursor2 = QueryCursor::new();
    let mut deferred: Vec<Deferred> = Vec::new();

    // We need at least one local symbol to be a source
    if local_ids.is_empty() { return Ok(deferred); }
    // Use the first symbol as a fallback source (best-effort attribution)
    let fallback_source = *local_ids.values().next().unwrap();

    {
        let mut matches = cursor2.matches(&call_q, root, src);
        while let Some(m) = matches.next() {
            let Some(idx) = call_name_idx else { continue };
            let Some(cap) = m.captures.iter().find(|c| c.index == idx) else { continue };
            let call_name = match cap.node.utf8_text(src) {
                Ok(s) if !s.is_empty() => s.to_string(),
                _ => continue,
            };
            if is_noise(&call_name) { continue; }

            // Determine source symbol (symbol whose range contains this call's start)
            let call_row = cap.node.start_position().row;
            let source_id = find_enclosing_symbol(conn, file_id, call_row as i64)
                .unwrap_or(fallback_source);

            if let Some(&target_id) = local_ids.get(&call_name) {
                if source_id != target_id {
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO edges (source_id, target_id, kind, confidence)
                         VALUES (?1, ?2, 'CALLS', 'exact')",
                        rusqlite::params![source_id, target_id],
                    );
                    stats.edges_intra += 1;
                }
            } else {
                deferred.push(Deferred { source_id, target_name: call_name, edge_kind: "CALLS" });
            }
        }
    }

    // ── Bridge Query: framework patterns (Approach 1) ──────────────────────────
    // Run only if this language has a built-in bridge_query (DI, decorators, etc.)
    if let Some(bridge_src) = cfg.bridge_query {
        if let Ok(bridge_q) = Query::new(&cfg.ts_language, bridge_src) {
            let name_idx = bridge_q.capture_names().iter()
                .position(|n| *n == "name").map(|i| i as u32);
            let ann_idx = bridge_q.capture_names().iter()
                .position(|n| *n == "_ann").map(|i| i as u32);
            let fn_idx = bridge_q.capture_names().iter()
                .position(|n| *n == "_fn").map(|i| i as u32);

            let mut cursor3 = QueryCursor::new();
            let mut matches3 = cursor3.matches(&bridge_q, root, src);
            while let Some(m) = matches3.next() {
                // Get the target @name capture
                let Some(name_cap_i) = name_idx else { continue };
                let Some(name_cap) = m.captures.iter().find(|c| c.index == name_cap_i) else { continue };
                let target_name = match name_cap.node.utf8_text(src) {
                    Ok(s) if !s.is_empty() => s.to_string(),
                    _ => continue,
                };

                // Filter: for Java, only emit if @_ann is a known DI annotation
                if let Some(ann_i) = ann_idx {
                    if let Some(ann_cap) = m.captures.iter().find(|c| c.index == ann_i) {
                        let ann = ann_cap.node.utf8_text(src).unwrap_or("");
                        const DI_ANNS: &[&str] = &["Autowired","Inject","Resource","Provides","Bean"];
                        if !DI_ANNS.contains(&ann) {
                            continue; // skip non-DI annotations
                        }
                    }
                }

                // Filter: for TS/JS, only emit if @_fn == "require"
                if let Some(fn_i) = fn_idx {
                    if let Some(fn_cap) = m.captures.iter().find(|c| c.index == fn_i) {
                        let fn_name = fn_cap.node.utf8_text(src).unwrap_or("");
                        if fn_name != "require" {
                            continue; // skip non-require dynamic calls
                        }
                    }
                }

                let call_row = name_cap.node.start_position().row;
                let source_id = find_enclosing_symbol(conn, file_id, call_row as i64)
                    .unwrap_or(fallback_source);
                deferred.push(Deferred {
                    source_id,
                    target_name,
                    edge_kind: cfg.bridge_edge_kind,
                });
            }
        }
    }

    Ok(deferred)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the symbol in this file whose range contains the given row.
fn find_enclosing_symbol(conn: &Connection, file_id: i64, row: i64) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM symbols WHERE file_id = ?1
         AND start_line <= ?2 AND end_line >= ?2
         ORDER BY (end_line - start_line) ASC LIMIT 1",
        rusqlite::params![file_id, row + 1],
        |r| r.get(0),
    ).ok()
}

/// Extract doc comment lines immediately above `start_row` (0-indexed).
fn extract_doc(content: &str, start_row: usize) -> Option<String> {
    if start_row == 0 { return None; }
    let lines: Vec<&str> = content.lines().collect();
    let mut doc = Vec::new();
    let mut i = start_row.saturating_sub(1);
    loop {
        let line = lines.get(i)?.trim();
        if line.starts_with("///") || line.starts_with("//!") {
            doc.push(line.trim_start_matches("///").trim_start_matches("//!").trim());
        } else if line.starts_with("/**") || line.starts_with("* ") || line == "*/" || line.starts_with("# ") {
            doc.push(line.trim_start_matches("/**").trim_start_matches("* ")
                .trim_end_matches("*/").trim_start_matches("# ").trim());
        } else {
            break;
        }
        if i == 0 { break; }
        i -= 1;
    }
    if doc.is_empty() { return None; }
    doc.reverse();
    Some(doc.join(" ").trim().to_string())
}

/// camelCase / snake_case → space-separated lowercase tokens for BM25.
fn tokenize_name(name: &str) -> Option<String> {
    if !name.chars().any(|c| c.is_uppercase() || c == '_') {
        return None;
    }
    let mut tokens = String::new();
    let mut last_upper = false;
    for (i, ch) in name.char_indices() {
        if ch == '_' {
            tokens.push(' ');
            last_upper = false;
        } else if ch.is_uppercase() && i > 0 && !last_upper {
            tokens.push(' ');
            tokens.extend(ch.to_lowercase());
            last_upper = true;
        } else {
            tokens.extend(ch.to_lowercase());
            last_upper = ch.is_uppercase();
        }
    }
    let r = tokens.trim().to_string();
    if r == name.to_lowercase() { None } else { Some(r) }
}

fn is_test_path(path: &str) -> bool {
    let p = path.to_lowercase();
    p.contains("/test") || p.contains("_test.") || p.contains(".test.")
        || p.contains("_spec.") || p.contains(".spec.")
}

// ---------------------------------------------------------------------------
// AGENTS.md writer
// ---------------------------------------------------------------------------

pub fn write_agents_md(root: &Path, _files: usize, symbols: usize) {
    let agents_path = root.join("AGENTS.md");
    let existing = std::fs::read_to_string(&agents_path).unwrap_or_default();
    
    let onboarding = if symbols > 0 {
        format!(
            "\n\
            ### 🔴 First-time Setup — MANDATORY Knowledge Layer Bootstrap\n\
            > fog-context indexed Layer 1 (Physical: {symbols} symbols). \
            Semantic Layers 2-4 are empty — Knowledge Score: 0/100.\n\
            > Complete these steps **once** to unlock full intelligence:\n\
            \n\
            ```\n\
            Step 1 - Layer 2 (Business Domains): fog_assign\n\
            Step 2 - Layer 3 (Constraints): fog_constraints\n\
            Step 3 - Layer 4 (Decisions): fog_decisions\n\
            ```\n"
        )
    } else {
        String::new()
    };

    let section = format!(
        "<!-- fog-context -->\n\
         ## fog-context MCP - Agent Instructions\n\
         \n\
         > [!WARNING]\n\
         > **⚠️ ALL `fog_*` commands are MCP Tools.** Do NOT run them via bash/shell.\n\
         \n\
         ### MANDATORY PROTOCOL\n\
         1. **Call `fog_brief` First:** Always check index health.\n\
         2. **Before Editing:** Run `fog_impact({{ \"target\": \"<symbol>\" }})` to check blast radius.\n\
         3. **After Editing:** Run `fog_decisions` to log WHY the code was changed.\n\
         \n\
         ### MCP Tools\n\
         - **Orient:** `fog_domains`, `fog_lookup`\n\
         - **Understand:** `fog_inspect`, `fog_trace`\n\
         - **Verify:** `fog_impact`\n\
         - **Record:** `fog_decisions`\n\
         \n\
         ⚠️ **Anti-Blackbox Rule:** You MUST NOT bypass cross-validation. \n\
         Even with a detailed prompt, ALWAYS verify real codebase state via `fog_inspect` before modifying code.\n\
         {onboarding}<!-- /fog-context -->"
    );

    // Replace existing block or append
    let start_idx = existing.find("<!-- fog-context -->");
    let end_idx = existing.find("<!-- /fog-context -->").map(|i| i + "<!-- /fog-context -->".len());
    
    let new_content = match (start_idx, end_idx) {
        (Some(start), Some(end)) if start < end => {
            let mut s = String::new();
            s.push_str(&existing[..start]);
            s.push_str(&section);
            s.push_str(&existing[end..]);
            s
        }
        _ => {
            if existing.is_empty() {
                section
            } else {
                let pad = if existing.ends_with("\n\n") { "" } else if existing.ends_with('\n') { "\n" } else { "\n\n" };
                format!("{existing}{pad}{section}")
            }
        }
    };
    
    let _ = std::fs::write(&agents_path, new_content);
}

fn log_global_parser_error(lang: &str, err: &str) {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let log_dir = std::path::PathBuf::from(home).join(".fog").join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = log_dir.join("parser_errors.log");
    
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    let now = format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z");

    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_file) {
        let _ = writeln!(f, "[{}] [{}]: {}", now, lang, err);
    }
}
