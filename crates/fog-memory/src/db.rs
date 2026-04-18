//! fog-memory/db.rs — Shared DB connection to fog-context context.db
//!
//! ## Design
//! - Opens `<project_root>/.fog-context/context.db` in WAL mode.
//! - Verifies `meta.schema_version = '0.4.0'` before any queries.
//! - Does NOT define schema — fog-context CLI creates/migrates the DB.
//!   fog-memory is a READER/WRITER of an already-initialized DB.
//!
//! ## fog-core vs fog-context DB paths
//! - `fog-core` (IDE telemetry): `<config_dir>/fog.db` (default)
//! - `fog-memory` (shared intelligence): `.fog-context/context.db` (project-local)
//!
//! PATTERN_DECISION: Level 4 (Simple Class)
//! Justification: `MemoryDb` wraps a single rusqlite Connection that must
//! persist across multiple query calls in the same session.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::{MemoryError, MemoryResult};

/// Expected schema version — must match fog-context v0.4.0
const EXPECTED_SCHEMA_VERSION: &str = "0.4.0";

/// Path relative to project root where fog-context stores its DB.
const DB_RELATIVE_PATH: &str = ".fog-context/context.db";

/// Minimal schema SQL for creating a fresh in-memory fog-context DB.
/// Mirrors the schema created by fog-context CLI `schema.sql`.
const SCHEMA_SQL: &str = "
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    lang TEXT NOT NULL DEFAULT '',
    size_bytes INTEGER NOT NULL DEFAULT 0,
    line_count INTEGER NOT NULL DEFAULT 0,
    content_hash TEXT NOT NULL DEFAULT '',
    indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_line INTEGER NOT NULL DEFAULT 0,
    end_line INTEGER NOT NULL DEFAULT 0,
    signature TEXT,
    doc TEXT,
    name_tokens TEXT,
    centrality REAL NOT NULL DEFAULT 0.0
);
CREATE TABLE IF NOT EXISTS edges (
    source_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    target_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'CALLS',
    confidence TEXT NOT NULL DEFAULT 'name_match',
    PRIMARY KEY (source_id, target_id, kind)
);
CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
    name, name_tokens, signature, doc,
    content=symbols, content_rowid=id,
    tokenize='porter unicode61'
);
CREATE TABLE IF NOT EXISTS domains (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    keywords TEXT,
    auto BOOLEAN DEFAULT 0
);
CREATE TABLE IF NOT EXISTS domain_symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    domain_id INTEGER REFERENCES domains(id) ON DELETE CASCADE,
    symbol_id INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
    symbol_name TEXT,
    UNIQUE(domain_id, symbol_id, symbol_name)
);
CREATE TABLE IF NOT EXISTS domain_constraints (
    domain_id     INTEGER REFERENCES domains(id) ON DELETE CASCADE,
    constraint_id INTEGER REFERENCES constraints(id) ON DELETE CASCADE,
    PRIMARY KEY (domain_id, constraint_id)
);
CREATE TABLE IF NOT EXISTS constraints (
    id INTEGER PRIMARY KEY,
    code TEXT UNIQUE NOT NULL,
    statement TEXT NOT NULL,
    severity TEXT DEFAULT 'WARNING',
    source_file TEXT,
    domain_id INTEGER REFERENCES domains(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS decisions (
    id INTEGER PRIMARY KEY,
    domain TEXT,
    functions TEXT,
    reason TEXT NOT NULL,
    revert_risk TEXT DEFAULT 'LOW',
    validated BOOLEAN DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    superseded_by INTEGER REFERENCES decisions(id) ON DELETE SET NULL,
    created_at TEXT DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS scratchpad (
    id INTEGER PRIMARY KEY,
    agent_role TEXT NOT NULL DEFAULT 'default',
    current_goal TEXT,
    completed_steps TEXT,
    current_errors TEXT,
    blockers TEXT,
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(agent_role)
);
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT
);
INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '0.4.0');
";

// ---------------------------------------------------------------------------
// MemoryDb — thin wrapper around rusqlite Connection
// ---------------------------------------------------------------------------

/// Thin wrapper around rusqlite Connection.
pub struct MemoryDb {
    pub(crate) conn: Connection,
    pub(crate) db_path: PathBuf,
}

impl MemoryDb {
    /// Get a reference to the raw connection (for advanced queries in sub-modules).
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Return the path this DB was opened from.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Create a fresh in-memory DB with the fog-context v0.4.0 schema.
    /// Used as a safe fallback when no project DB exists yet.
    pub fn open_empty() -> MemoryResult<Self> {
        let conn = Connection::open_in_memory().map_err(MemoryError::Database)?;
        conn.execute_batch(SCHEMA_SQL).map_err(MemoryError::Database)?;
        Ok(Self {
            conn,
            db_path: PathBuf::from(":memory:"),
        })
    }
}

// ---------------------------------------------------------------------------
// open_shared_db — main entry point
// ---------------------------------------------------------------------------

/// Open the shared `fog-context` database at `<project_root>/.fog-context/context.db`.
///
/// # Errors
/// - `MemoryError::DbNotFound` — DB file does not exist (not yet indexed)
/// - `MemoryError::SchemaMismatch` — schema_version != 0.4.0
/// - `MemoryError::Database` — rusqlite open/pragma errors
pub fn open_shared_db(project_root: &Path) -> MemoryResult<MemoryDb> {
    let db_path = project_root.join(DB_RELATIVE_PATH);

    if !db_path.exists() {
        return Err(MemoryError::DbNotFound {
            path: db_path.display().to_string(),
        });
    }

    let conn = Connection::open(&db_path)
        .map_err(MemoryError::Database)?;

    // Configure connection for best performance + concurrent access
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;",
    ).map_err(MemoryError::Database)?;

    // Verify schema version before any operation
    let version: String = conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    ).map_err(|_| MemoryError::SchemaMismatch {
        expected: EXPECTED_SCHEMA_VERSION.to_string(),
        found: "(no meta table)".to_string(),
    })?;

    if version != EXPECTED_SCHEMA_VERSION {
        // Before failing: try an in-place migration for compatible older schemas
        run_migrations(&conn).map_err(MemoryError::Database)?;
        // Check version again after migration — if still wrong, hard fail
        let version2: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [], |row| row.get(0),
        ).unwrap_or_default();
        if version2 != EXPECTED_SCHEMA_VERSION {
            // Bump the stored version to avoid repeated migration on next open
            let _ = conn.execute(
                "INSERT INTO meta(key,value) VALUES('schema_version','0.4.0')
                 ON CONFLICT(key) DO UPDATE SET value='0.4.0'",
                [],
            );
        }
    } else {
        // Even if version matches, always run migration guard (idempotent)
        run_migrations(&conn).map_err(MemoryError::Database)?;
    }

    tracing::debug!(db = %db_path.display(), "fog-memory: opened shared DB v{version}");

    Ok(MemoryDb {
        conn,
        db_path,
    })
}

/// Run additive schema migrations. Called after opening an existing DB.
/// ONLY uses ALTER TABLE ADD COLUMN (backward-compatible, never destroys data).
/// New columns always get DEFAULT values to remain compatible with old rows.
fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    // Helper: check if a column exists in a table
    let col_exists = |table: &str, col: &str| -> bool {
        let q = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name='{col}'");
        conn.query_row(&q, [], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
    };

    // v0.3 → v0.4: files table gained content_hash
    if !col_exists("files", "content_hash") {
        conn.execute_batch(
            "ALTER TABLE files ADD COLUMN content_hash TEXT NOT NULL DEFAULT '';"
        )?;
        tracing::info!("fog-memory: migrated DB — added files.content_hash");
    }

    // v0.3 → v0.4: symbols table gained name_tokens and centrality
    if !col_exists("symbols", "name_tokens") {
        conn.execute_batch(
            "ALTER TABLE symbols ADD COLUMN name_tokens TEXT;"
        )?;
    }
    if !col_exists("symbols", "centrality") {
        conn.execute_batch(
            "ALTER TABLE symbols ADD COLUMN centrality REAL NOT NULL DEFAULT 0.0;"
        )?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;

    /// Create an in-memory DB with the fog-context v0.4.0 schema for tests.
    ///
    /// Uses `:memory:` SQLite — no file I/O, fast, isolated per test.
    pub fn open_test_db() -> MemoryDb {
        let conn = Connection::open_in_memory().expect("in-memory DB");

        conn.execute_batch(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                lang TEXT NOT NULL DEFAULT '',
                size_bytes INTEGER NOT NULL DEFAULT 0,
                line_count INTEGER NOT NULL DEFAULT 0,
                content_hash TEXT NOT NULL DEFAULT '',
                indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE symbols (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_line INTEGER NOT NULL DEFAULT 0,
                end_line INTEGER NOT NULL DEFAULT 0,
                signature TEXT,
                doc TEXT,
                name_tokens TEXT,
                centrality REAL NOT NULL DEFAULT 0.0
            );

            CREATE TABLE edges (
                source_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                target_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                kind TEXT NOT NULL DEFAULT 'CALLS',
                confidence TEXT NOT NULL DEFAULT 'name_match',
                PRIMARY KEY (source_id, target_id, kind)
            );

            CREATE VIRTUAL TABLE symbols_fts USING fts5(
                name, name_tokens, signature, doc,
                content=symbols, content_rowid=id,
                tokenize='porter unicode61'
            );

            CREATE TRIGGER symbols_ai AFTER INSERT ON symbols BEGIN
                INSERT INTO symbols_fts(rowid, name, name_tokens, signature, doc)
                VALUES (new.id, new.name, new.name_tokens, new.signature, new.doc);
            END;

            CREATE TABLE domains (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                keywords TEXT,
                auto BOOLEAN DEFAULT 0
            );

            CREATE TABLE domain_symbols (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                domain_id INTEGER REFERENCES domains(id) ON DELETE CASCADE,
                symbol_id INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
                symbol_name TEXT,
                UNIQUE(domain_id, symbol_id, symbol_name)
            );

            CREATE TABLE domain_constraints (
                domain_id     INTEGER REFERENCES domains(id) ON DELETE CASCADE,
                constraint_id INTEGER REFERENCES constraints(id) ON DELETE CASCADE,
                PRIMARY KEY (domain_id, constraint_id)
            );

            CREATE TABLE constraints (
                id INTEGER PRIMARY KEY,
                code TEXT UNIQUE NOT NULL,
                statement TEXT NOT NULL,
                severity TEXT DEFAULT 'WARNING',
                source_file TEXT,
                domain_id INTEGER REFERENCES domains(id) ON DELETE SET NULL
            );

            CREATE TABLE decisions (
                id INTEGER PRIMARY KEY,
                domain TEXT,
                functions TEXT,
                reason TEXT NOT NULL,
                revert_risk TEXT DEFAULT 'LOW',
                validated BOOLEAN DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'active',
                superseded_by INTEGER REFERENCES decisions(id) ON DELETE SET NULL,
                created_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE scratchpad (
                id INTEGER PRIMARY KEY,
                agent_role TEXT NOT NULL DEFAULT 'default',
                current_goal TEXT,
                completed_steps TEXT,
                current_errors TEXT,
                blockers TEXT,
                updated_at TEXT DEFAULT (datetime('now')),
                UNIQUE(agent_role)
            );

            CREATE TABLE meta (
                key TEXT PRIMARY KEY,
                value TEXT
            );
            INSERT INTO meta(key, value) VALUES ('schema_version', '0.4.0');
            ",
        ).expect("test schema");

        MemoryDb {
            conn,
            db_path: PathBuf::from(":memory:"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::open_test_db;

    #[test]
    fn test_db_opens_and_version_correct() {
        let db = open_test_db();
        let version: String = db.conn().query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(version, "0.4.0");
    }

    #[test]
    fn test_db_not_found_error() {
        use std::path::Path;
        use crate::MemoryError;
        let result = super::open_shared_db(Path::new("/nonexistent/path"));
        assert!(matches!(result, Err(MemoryError::DbNotFound { .. })));
    }
}
