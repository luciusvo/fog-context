//! fog-memory - FoG IDE Native Intelligence Engine
//!
//! **Track 2** of the Dual-Track Architecture (ADR-001).
//!
//! Shares `context.db` with `fog-context` (Track 1, TypeScript/MCP) but adds
//! **enforced compliance** - the AI agent cannot skip Layers 2-5 maintenance
//! because `fog-harness/gateway_loop` intercepts and injects required calls.
//!
//! ## Architecture
//!
//! ```text
//! fog-harness (gateway_loop)
//!      │
//!      ├──▶ fog-memory::QueryEngine ← direct rusqlite reads (search, context, impact)
//!      │
//!      └──▶ fog-memory::WriteEngine ← direct rusqlite writes (decisions, domains, scratchpad)
//! ```
//!
//! ## Schema compatibility
//!
//! fog-memory reads the **fog-context v0.4.0 canonical schema** (schema.sql).
//! `db::open_shared_db()` verifies the schema_version before any queries.
//!
//! PATTERN_DECISION: Level 4 (Simple Class)
//! Justification: `MemoryEngine` needs persistent DB connection across calls.
//! All individual operations are Level 1 pure functions delegating to the engine.

pub mod compressor;
pub mod db;
pub mod query;
pub mod write;

pub use db::{MemoryDb, open_shared_db};
pub use query::DomainDetail;

use std::path::Path;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: String, found: String },

    #[error("DB not found at {path} - run `fog-context index` first")]
    DbNotFound { path: String },

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type MemoryResult<T> = Result<T, MemoryError>;

// ---------------------------------------------------------------------------
// MemoryEngine trait - unified interface used by fog-harness
// ---------------------------------------------------------------------------

/// Unified interface for all fog-memory operations.
///
/// Implemented by `MemoryDb`. Can be mocked in tests.
///
/// PATTERN_DECISION: Level 4 (trait-based DI)
/// Justification: allows fog-harness to mock in unit tests without a real DB.
/// Note: `Send` only (not `Sync`) - rusqlite::Connection is not thread-safe.
///   Wrap in `Arc<Mutex<MemoryDb>>` for multi-threaded access.
pub trait MemoryEngine: Send {
    // ── Read ──
    fn search(&self, query: &str, limit: usize, kind: Option<&str>) -> MemoryResult<Vec<query::SearchHit>>;
    fn context_symbol(&self, name: &str) -> MemoryResult<Option<query::SymbolContext>>;
    fn impact(&self, target: &str, depth: u32, direction: &str) -> MemoryResult<query::ImpactResult>;
    fn route_map(&self, entry: &str, depth: u32, direction: &str, token_budget: Option<usize>) -> MemoryResult<query::RouteMapResult>;
    fn domain_catalog(&self) -> MemoryResult<Vec<query::DomainInfo>>;
    fn knowledge_score(&self) -> MemoryResult<query::KnowledgeScore>;

    // ── Write ──
    fn record_decision(&self, args: write::RecordDecisionArgs) -> MemoryResult<i64>;
    fn define_domain(&self, args: write::DefineDomainArgs) -> MemoryResult<()>;
    fn scratchpad_get(&self, role: &str) -> MemoryResult<Option<write::ScratchpadState>>;
    fn scratchpad_update(&self, role: &str, state: write::ScratchpadUpdateArgs) -> MemoryResult<()>;
}

// ---------------------------------------------------------------------------
// Convenience: open engine from project root
// ---------------------------------------------------------------------------

/// Open a `MemoryDb` from a project root directory.
///
/// Looks for `<root>/.fog-context/context.db` - the standard location used
/// by both `fog-context` CLI and `fog-core` IDE server.
///
/// Returns `Err(MemoryError::DbNotFound)` if the DB does not exist (not yet indexed).
pub fn open_from_project(root: &Path) -> MemoryResult<MemoryDb> {
    open_shared_db(root)
}

/// Create the DB if it doesn't exist, then open it.
///
/// Used by **CLI `index` subcommand** and **MCP `fog_scan` first-run** to solve the
/// Chicken-and-Egg problem: the indexer needs a DB to write to, but the DB
/// doesn't exist until something creates it.
///
/// ## What it does
/// 1. Creates `.fog-context/` directory if missing.
/// 2. If `context.db` doesn't exist: initialises it with the canonical schema
///    (equivalent to running `CREATE TABLE` migrations from SCHEMA_SQL).
/// 3. Opens and returns the DB via `open_shared_db()`.
///
/// Idempotent - safe to call on an already-initialised project.
pub fn create_or_open_db(root: &Path) -> MemoryResult<MemoryDb> {
    let fog_dir = root.join(".fog-context");
    let db_path = fog_dir.join("context.db");

    if !db_path.exists() {
        // Bootstrap: create directory + fresh schema
        std::fs::create_dir_all(&fog_dir).map_err(|e| {
            MemoryError::Database(rusqlite::Error::InvalidPath(
                format!("cannot create .fog-context/: {e}").into()
            ))
        })?;

        // Write schema into a brand-new SQLite file
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(MemoryError::Database)?;
        conn.execute_batch(db::SCHEMA_SQL)
            .map_err(MemoryError::Database)?;

        eprintln!("[fog-context] Initialised new DB at {}", db_path.display());

        // #7: Guard commits — inject .fog-context/ into project .gitignore
        inject_gitignore(root);
    }

    // #2: Surface schema mismatch with a user-friendly, actionable message
    open_shared_db(root).map_err(|e| {
        if let MemoryError::SchemaMismatch { expected, found } = &e {
            MemoryError::Database(rusqlite::Error::InvalidPath(
                format!(
                    "[fog-context] SCHEMA INCOMPATIBLE: expected v{expected}, found v{found}.\n\
                     Action: delete .fog-context/context.db then call fog_scan to reindex.\n\
                     Command: rm -f .fog-context/context.db"
                ).into()
            ))
        } else {
            e
        }
    })
}

/// #7: Append `.fog-context/` entry to the project's .gitignore if present and not already included.
/// Silently skips if .gitignore doesn't exist or is read-only.
fn inject_gitignore(root: &Path) {
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() { return; }

    let Ok(content) = std::fs::read_to_string(&gitignore) else { return; };
    // Already present — don't duplicate
    if content.lines().any(|l| l.trim() == ".fog-context/" || l.trim() == ".fog-context") {
        return;
    }

    // Ignore the DB/cache but keep hint files versioned with the project
    let append = "\n# fog-context agentic index (auto-generated)\n.fog-context/\n!.fog-context/hints/\n";
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&gitignore) {
        use std::io::Write;
        let _ = file.write_all(append.as_bytes());
        eprintln!("[fog-context] Added .fog-context/ to .gitignore (hints/ excluded)");
    }
}

