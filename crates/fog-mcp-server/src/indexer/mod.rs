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
    pub elapsed_ms: u128,
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

    // E7: Progress feedback to stderr (visible in CLI, silent to MCP JSON-RPC stdout)
    eprintln!("[fog] 1/5 🔍 Walking files: {}", project_root.display());

    // Step 1: Walk files (gitignore-aware)
    let scanned = walker::walk_project(project_root);
    if scanned.is_empty() {
        return ToolCallResult::ok("⚠️  No source files found. Is the project path correct?");
    }
    eprintln!("[fog] 2/5 🌲 Found {} files. Starting parsing (Pass 1, full={})...", scanned.len(), full);

    // Step 2: Run two-pass indexer
    let stats = match ingest::run_two_pass(project_root, db, &scanned, full) {
        Ok(s) => s,
        Err(e) => return ToolCallResult::err(format!("fog_scan error: {e}")),
    };
    eprintln!("[fog] ✓ Pass 1 done: {} symbols, {} intra-file edges", stats.symbols_created, stats.edges_intra);
    eprintln!("[fog] ✓ Pass 2 done: {} cross-file edges", stats.edges_cross);

    let elapsed = start.elapsed().as_millis();

    // Step 3: Write AGENTS.md workflow guide
    ingest::write_agents_md(project_root, scanned.len(), stats.symbols_created, elapsed);

    // Step 4: Register project in global ~/.fog/registry.json
    // Fix 2b: ensure fog_id is generated NOW (eager, before registry upsert)
    // so it's available in the response even if registry call fails.
    let fog_id = crate::registry::ensure_project_id(&project_root.to_string_lossy());
    // C1: Record the binary version that indexed this project → fog_brief can detect stale index
    {
        let mut cfg = crate::registry::ProjectConfig::load(project_root);
        cfg.indexer_version = Some(env!("CARGO_PKG_VERSION").to_string());
        cfg.save(project_root);
    }
    // E2 fix: Query TOTAL symbols from DB (not delta) so re-scans don't show symbol_count=0
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

    // A3: Large repo advisory — scanned.len() > 1000 files
    let large_repo_warning = if scanned.len() > 1000 {
        format!(
            "\n\n> ⚠️ **Large repo ({} files detected)**\n\
             > For faster future updates, prefer CLI indexing:\n\
             > ```bash\n\
             > fog-mcp-server index --project {}\n\
             > ```\n\
             > This runs with visible progress and avoids MCP timeouts.",
            scanned.len(),
            project_root.display()
        )
    } else {
        String::new()
    };

    // #12b: Up-to-date detection — if nothing changed AND no errors, say so clearly
    // "0 indexed" is ambiguous: it could mean OK or catastrophic failure.
    if stats.files_indexed == 0 && stats.files_deleted == 0 && stats.query_errors.is_empty() {
        let total_symbols = db.total_symbols();
        return ToolCallResult::ok(format!(
            "✅ **Up-to-date** — No changes detected.\n\
             - **Project:** {}\n\
             - **Files checked:** {total} (all match indexed snapshot)\n\
             - **Total symbols in graph:** {total_symbols}\n\
             - **Graph status:** Reliable ✓\n\n\
             The knowledge graph is current. No re-indexing needed.\n\
             Use fog_lookup, fog_inspect, fog_impact to explore.",
            project_root.display(),
            total = scanned.len(),
        ));
    }

    // Warn about any query compile errors so agents see them immediately
    let warnings = if stats.query_errors.is_empty() {
        String::new()
    } else {
        let total_symbols = db.total_symbols();
        if total_symbols == 0 {
            format!(
                "\n\n> [!CRITICAL]\n> 🔴 **Toàn bộ hệ thống không thể trích xuất được code.** Phát hiện lỗi nghiêm trọng từ Parsers:\n{}",
                stats.query_errors.iter().map(|e| format!("> - `{e}`")).collect::<Vec<_>>().join("\n")
            )
        } else {
            format!(
                "\n\n> [!WARNING]\n> ⚠️ **Parser errors** - Có một số lỗi cú pháp khiến một phần source code không lấy được dữ liệu:\n{}",
                stats.query_errors.iter().map(|e| format!("> - `{e}`")).collect::<Vec<_>>().join("\n")
            )
        }
    };

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
         - **Elapsed:** {elapsed}ms\n\
         \n\
         Next: `fog_lookup`, `fog_inspect`, `fog_impact` to explore the graph.{warnings}{large_repo}",
        fog_id = fog_id,
        path = project_root.display(),
        total = scanned.len(),
        indexed = stats.files_indexed,
        deleted = stats.files_deleted,
        syms = stats.symbols_created,
        intra = stats.edges_intra,
        cross = stats.edges_cross,
        total_edges = stats.edges_intra + stats.edges_cross,
        large_repo = large_repo_warning,
    ))
}

