//! fog-mcp-server/src/indexer/mod.rs
//!
//! Phase 4B: Tree-sitter-powered incremental codebase indexer.
//!
//! Two-pass strategy (mirrors TypeScript fog-context-repo):
//!   Pass 1 - Walk files, parse AST, extract symbols + intra-file edges.
//!             Store unresolved cross-file calls in a deferred queue.
//!   Pass 2 - Resolve deferred calls against the full symbol index.
//!             Insert cross-file CALLS edges with confidence scores.
//!
//! PATTERN_DECISION: Level 2 (Composition)
//!   run_scan() = walk_files ∘ parse_files ∘ ingest ∘ resolve_cross_file
//!   Each step is a pure transformation.

pub mod hints;
pub mod ingest;
pub mod langs;
pub mod walker;

use std::path::Path;

use crate::protocol::ToolCallResult;
use crate::registry::Registry;

/// Indexing statistics returned by run_scan.
#[derive(Debug, Default)]
pub struct IndexStats {
    pub files_total: usize,
    pub files_indexed: usize,
    pub files_deleted: usize,
    pub symbols_created: usize,
    pub edges_intra: usize,
    pub edges_cross: usize,
    /// Query compile errors per language (non-empty = some language was not indexed).
    /// Example: ["tsx: QueryError { kind: Structure }"]
    pub query_errors: Vec<String>,
}

/// Entry point called by fog_scan tool handler.
///
/// `full = true` → ignore checksums, re-parse all files.
/// `full = false` → incremental (only changed files).
pub fn run_scan(
    project_root: &Path,
    db: &fog_memory::MemoryDb,
    full: bool,
) -> ToolCallResult {
    let start = std::time::Instant::now();

    let scanned = match phase_walk(project_root) {
        Ok(files) => files,
        Err(e) => return ToolCallResult::ok(e),
    };

    if scanned.len() > 1000 && db.total_symbols() == 0 {
        let fog_id = crate::registry::ensure_project_id(&project_root.to_string_lossy());
        return ToolCallResult::ok(format!(
            "⚠️ **Large codebase detected ({count} files)**\n\n\
             MCP scan may timeout. Use CLI for initial indexing:\n\
             ```bash\n\
             ~/.fog/bin/fog-mcp-server index --project {path}\n\
             ```\n\n\
             After CLI completes, verify with:\n\
             ```\n\
             fog_brief({{ \"project\": \"{fog_id}\" }})\n\
             ```",
            count = scanned.len(),
            path = project_root.display(),
            fog_id = fog_id,
        ));
    }

    let stats = match phase_parse(project_root, db, &scanned, full) {
        Ok(s) => s,
        Err(e) => return ToolCallResult::err(e),
    };

    let elapsed = start.elapsed().as_millis();

    phase_embed(db, stats.symbols_created);

    let fog_id = phase_register(project_root, db, scanned.len(), stats.symbols_created);

    phase_format_result(project_root, db, &stats, &fog_id, scanned.len(), elapsed)
}

fn phase_walk(project_root: &Path) -> Result<Vec<walker::ScannedFile>, String> {
    eprintln!("[fog] 1/5 🔍 Walking files: {}", project_root.display());
    let scanned = walker::walk_project(project_root);
    if scanned.is_empty() {
        return Err("⚠️  No source files found. Is the project path correct?".to_string());
    }
    Ok(scanned)
}

fn phase_parse(
    project_root: &Path,
    db: &fog_memory::MemoryDb,
    files: &[walker::ScannedFile],
    full: bool,
) -> Result<IndexStats, String> {
    eprintln!("[fog] 2/5 🌲 Found {} files. Starting parsing (Pass 1, full={})...", files.len(), full);
    let stats = ingest::run_two_pass(project_root, db, files, full).map_err(|e| format!("fog_scan error: {e}"))?;
    eprintln!("[fog] ✓ Pass 1 done: {} symbols, {} intra-file edges", stats.symbols_created, stats.edges_intra);
    eprintln!("[fog] ✓ Pass 2 done: {} cross-file edges", stats.edges_cross);
    Ok(stats)
}

#[cfg(feature = "embedding")]
fn phase_embed(db: &fog_memory::MemoryDb, _new_symbols_count: usize) {
    let _model = match crate::semantic::get_model() {
        Some(m) => m,
        None => {
            eprintln!("[fog] 3/5 ⏭️  Semantic model missing. Skipping ONNX embeddings.");
            return;
        }
    };
    
    eprintln!("[fog] 3/5 🧠 Generating embeddings for missing symbols...");
    
    let conn = db.conn();
    let mut stmt = match conn.prepare(
        "SELECT id, name, doc, signature FROM symbols WHERE id NOT IN (SELECT symbol_id FROM symbol_embeddings)"
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to prepare embedding query: {}", e);
            return;
        }
    };
    
    let rows = match stmt.query_map([], |row: &rusqlite::Row<'_>| {
        Ok((
            row.get::<usize, i64>(0)?,
            row.get::<usize, String>(1)?,
            row.get::<usize, Option<String>>(2)?,
            row.get::<usize, Option<String>>(3)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return,
    };
    
    let mut count = 0;
    for row_res in rows {
        let (id, name, doc, sig) = match row_res {
            Ok(r) => r,
            Err(_) => continue,
        };
        let mut text: String = name;
        if let Some(s) = sig { text.push_str(&format!(" {}", s)); }
        if let Some(d) = doc { text.push_str(&format!(" {}", d)); }
        
        if let Ok(vector) = crate::semantic::embed_text(&text) {
            let _ = db.insert_symbol_embeddings(id, &vector);
            count += 1;
        }
    }
    if count > 0 {
        eprintln!("[fog] ✓ Generated {} semantic embeddings", count);
    }
}

#[cfg(not(feature = "embedding"))]
fn phase_embed(_db: &fog_memory::MemoryDb, _new_symbols_count: usize) {}

fn phase_register(
    project_root: &Path,
    db: &fog_memory::MemoryDb,
    file_count: usize,
    symbol_count: usize,
) -> String {
    ingest::write_agents_md(project_root, file_count, symbol_count);
    let fog_id = crate::registry::ensure_project_id(&project_root.to_string_lossy());
    {
        let mut cfg = crate::registry::ProjectConfig::load(project_root);
        cfg.indexer_version = Some(env!("CARGO_PKG_VERSION").to_string());
        cfg.save(project_root);
    }
    {
        let name = project_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());
        let path = project_root.to_string_lossy().into_owned();
        let total_symbols = db.total_symbols();
        let mut reg = Registry::load();
        reg.upsert(name, path, total_symbols);
        eprintln!("[fog] Registry updated: {} total symbols", total_symbols);
    }
    fog_id
}

fn phase_format_result(
    project_root: &Path,
    db: &fog_memory::MemoryDb,
    stats: &IndexStats,
    fog_id: &str,
    file_count: usize,
    elapsed_ms: u128,
) -> ToolCallResult {
    let total_symbols = db.total_symbols();

    if stats.files_indexed == 0 && stats.files_deleted == 0 && stats.query_errors.is_empty() {
        return ToolCallResult::ok(format!(
            "✅ **Up-to-date** — No changes detected.\n\
             - **Project:** {}\n\
             - **Files checked:** {total} (all match indexed snapshot)\n\
             - **Total symbols in graph:** {total_symbols}\n\
             - **Graph status:** Reliable ✓\n\n\
             The knowledge graph is current. No re-indexing needed.\n\
             Use fog_lookup, fog_inspect, fog_impact to explore.",
            project_root.display(),
            total = file_count,
        ));
    }

    let large_repo_warning = if file_count > 1000 {
        format!(
            "\n\n> ⚠️ **Large repo ({} files detected)**\n\
             > For faster future updates, prefer CLI indexing:\n\
             > ```bash\n\
             > fog-mcp-server index --project {}\n\
             > ```\n\
             > This runs with visible progress and avoids MCP timeouts.",
            file_count,
            project_root.display()
        )
    } else {
        String::new()
    };

    let warnings = if stats.query_errors.is_empty() {
        String::new()
    } else {
        if total_symbols == 0 {
            format!(
                "\n\n> [!CRITICAL]\n> 🔴 **No symbols extracted.** All parsers failed. The knowledge graph is empty:\n{}",
                stats.query_errors.iter().map(|e| format!("> - `{e}`")).collect::<Vec<_>>().join("\n")
            )
        } else {
            format!(
                "\n\n> [!WARNING]\n> ⚠️ **Parser errors detected** — some source files could not be parsed. Partial graph only:\n{}",
                stats.query_errors.iter().map(|e| format!("> - `{e}`")).collect::<Vec<_>>().join("\n")
            )
        }
    };

    let mut bootstrap_hint = String::new();
    if let Ok(score) = db.knowledge_score() {
        if score.layer_score == 0 {
            bootstrap_hint = r#"
## 🟡 Knowledge Layers 2-4 are empty (Score: 0/100)

To unlock full codebase intelligence, tell your AI:
> "Populate fog-context knowledge layers for this codebase.
>  Read in multiple passes. Stop when a pass adds fewer than 3 new items,
>  or after 5 passes."

💡 This is read-heavy work. Use a **mid-tier model** (Gemini Flash, Claude Haiku)
   to save cost. A typical 1000-symbol project takes 2-3 minutes.
"#.to_string();
        }
    }

    ToolCallResult::ok(format!(
        "# fog_scan Complete ✅\n\
         \n\
         ## 🔑 Project Identity\n\
         | Field | Value |\n\
         |-------|-------|\n\
         | **fog_id** | `{fog_id}` |\n\
         | **Path** | `{path}` |\n\
         | **Config** | `{path}/.fog-context/config.toml` |\n\
         \n\
         > 💡 **Save this fog_id** — use it in ALL subsequent calls to ensure correct project routing:\n\
         > `{{ \"project\": \"{fog_id}\" }}`\n\
         \n\
         ## Indexing Results\n\
         - **Files:** {total} scanned, {indexed} indexed, {deleted} deleted\n\
         - **Symbols (new this scan):** {syms}\n\
         - **Edges:** {intra} intra-file + {cross} cross-file = {total_edges} total\n\
         - **Elapsed:** {elapsed}ms\n{bootstrap_hint}\
         \n\
         Next: `fog_lookup`, `fog_inspect`, `fog_impact` to explore the graph.{warnings}{large_repo}",
        fog_id = fog_id,
        path = project_root.display(),
        total = file_count,
        indexed = stats.files_indexed,
        deleted = stats.files_deleted,
        syms = stats.symbols_created,
        intra = stats.edges_intra,
        cross = stats.edges_cross,
        total_edges = stats.edges_intra + stats.edges_cross,
        elapsed = elapsed_ms,
        bootstrap_hint = bootstrap_hint,
        large_repo = large_repo_warning,
        warnings = warnings,
    ))
}

