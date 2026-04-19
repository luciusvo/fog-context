//! fog-mcp-server/src/indexer/mod.rs
//!
//! Phase 4B: Tree-sitter-powered incremental codebase indexer.
//!
//! Two-pass strategy (mirrors TypeScript fog-context-repo):
//!   Pass 1 — Walk files, parse AST, extract symbols + intra-file edges.
//!             Store unresolved cross-file calls in a deferred queue.
//!   Pass 2 — Resolve deferred calls against the full symbol index.
//!             Insert cross-file CALLS edges with confidence scores.
//!
//! PATTERN_DECISION: Level 2 (Composition)
//!   run_scan() = walk_files ∘ parse_files ∘ ingest ∘ resolve_cross_file
//!   Each step is a pure transformation.

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
    eprintln!("[fog] Walking files: {}", project_root.display());

    // Step 1: Walk files (gitignore-aware)
    let scanned = walker::walk_project(project_root);
    if scanned.is_empty() {
        return ToolCallResult::ok("⚠️  No source files found. Is the project path correct?");
    }
    eprintln!("[fog] Found {} files. Starting indexer (full={})...", scanned.len(), full);

    // Step 2: Run two-pass indexer
    let stats = match ingest::run_two_pass(project_root, db, &scanned, full) {
        Ok(s) => s,
        Err(e) => return ToolCallResult::err(format!("fog_scan error: {e}")),
    };
    eprintln!("[fog] Pass 1 done: {} symbols, {} intra-file edges", stats.symbols_created, stats.edges_intra);
    eprintln!("[fog] Pass 2 done: {} cross-file edges", stats.edges_cross);

    let elapsed = start.elapsed().as_millis();

    // Step 3: Write AGENTS.md workflow guide
    ingest::write_agents_md(project_root, scanned.len(), stats.symbols_created, elapsed);

    // Step 4: Register project in global ~/.fog/registry.json
    // E2 fix: Query TOTAL symbols from DB (not delta) so re-scans don't show symbol_count=0
    {
        let name = project_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());
        let path = project_root.to_string_lossy().into_owned();
        // Query actual total from DB — incremental scan may have symbols_created=0
        let total_symbols = db.total_symbols();
        let mut reg = Registry::load();
        reg.upsert(name, path, total_symbols);
        eprintln!("[fog] Registry updated: {} total symbols", total_symbols);
    }

    // Warn about any query compile errors so agents see them immediately
    let warnings = if stats.query_errors.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n> [!WARNING]\n> **Parser errors** — the following languages may have 0 symbols:\n{}",
            stats.query_errors.iter().map(|e| format!("> - `{e}`")).collect::<Vec<_>>().join("\n")
        )
    };

    ToolCallResult::ok(format!(
        "# fog_scan Complete ✅\n\
         - **Project:** {}\n\
         - **Files:** {total} scanned, {indexed} indexed, {deleted} deleted\n\
         - **Symbols (new):** {syms}\n\
         - **Edges:** {intra} intra-file + {cross} cross-file = {total_edges} total\n\
         - **Elapsed:** {elapsed}ms\n\
         \n\
         Next: Use fog_lookup, fog_inspect, fog_impact to explore.{warnings}",
        project_root.display(),
        total = scanned.len(),
        indexed = stats.files_indexed,
        deleted = stats.files_deleted,
        syms = stats.symbols_created,
        intra = stats.edges_intra,
        cross = stats.edges_cross,
        total_edges = stats.edges_intra + stats.edges_cross,
    ))
}

