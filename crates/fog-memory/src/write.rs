//! fog-memory/write.rs - Write operations (Layer 2-5)
//!
//! Implements 4 write operations mirroring fog-context TypeScript tools:
//!   record_decision()  → Layer 4 causality logger
//!   define_domain()    → Layer 2 domain registration
//!   scratchpad_get()   → Layer 5 task state read
//!   scratchpad_update()→ Layer 5 task state write
//!
//! PATTERN_DECISION: Level 1+2 (Pure Functions + Composition)
//! All functions: (&MemoryDb, args) → MemoryResult. Minimal side effects (DB writes).

use crate::{db::MemoryDb, MemoryResult};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Arg + result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordDecisionArgs {
    pub functions: Vec<String>,
    pub reason: String,
    pub domain: Option<String>,
    pub revert_risk: Option<String>, // "LOW" | "MEDIUM" | "HIGH"
    pub supersedes_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefineDomainArgs {
    pub name: String,
    pub keywords: Option<Vec<String>>,
    pub aliases: Option<Vec<String>>,
    pub symbols: Option<Vec<String>>,
    pub constraints: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScratchpadState {
    pub agent_role: String,
    pub current_goal: Option<String>,
    pub completed_steps: Vec<String>,
    pub current_errors: Vec<String>,
    pub blockers: Vec<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScratchpadUpdateArgs {
    pub current_goal: Option<String>,
    pub completed_steps: Option<Vec<String>>,
    pub current_errors: Option<Vec<String>>,
    pub blockers: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// record_decision() - Layer 4
// ---------------------------------------------------------------------------

impl MemoryDb {
    /// Record WHY code was changed - populates Layer 4 (Causality).
    ///
    /// Cross-validates that at least one of the listed functions exists in the index.
    /// If `supersedes_id` is set, marks the old decision as 'historical'.
    ///
    /// Returns the new decision's ID.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function with DB side effect)
    pub fn record_decision(&self, args: RecordDecisionArgs) -> MemoryResult<i64> {
        let conn = self.conn();

        // Cross-validate: check if any of the functions exist in the symbol index
        let placeholders: String = args.functions.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");

        let validated = if args.functions.is_empty() {
            false
        } else {
            let sql = format!("SELECT COUNT(*) FROM symbols WHERE name IN ({placeholders})");
            let mut stmt = conn.prepare(&sql).map_err(crate::MemoryError::Database)?;
            let count: i64 = stmt.query_row(
                rusqlite::params_from_iter(args.functions.iter()),
                |row| row.get(0),
            ).unwrap_or(0);
            count > 0
        };

        // Insert new decision
        conn.execute(
            "INSERT INTO decisions (domain, functions, reason, revert_risk, validated, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active')",
            rusqlite::params![
                args.domain,
                serde_json::to_string(&args.functions).map_err(crate::MemoryError::Json)?,
                args.reason,
                args.revert_risk.as_deref().unwrap_or("LOW"),
                validated as i64,
            ],
        ).map_err(crate::MemoryError::Database)?;

        let new_id = conn.last_insert_rowid();

        // Temporal chaining: mark old decision as historical
        if let Some(old_id) = args.supersedes_id {
            let updated = conn.execute(
                "UPDATE decisions SET status = 'historical', superseded_by = ?1 WHERE id = ?2",
                rusqlite::params![new_id, old_id],
            ).unwrap_or(0);

            if updated == 0 {
                tracing::warn!("supersedes_id {old_id} not found in decisions table");
            }
        }

        tracing::debug!(id = new_id, validated, "fog-memory: decision recorded");
        Ok(new_id)
    }

    // ---------------------------------------------------------------------------
    // define_domain() - Layer 2
    // ---------------------------------------------------------------------------

    /// Register a business domain and link symbols to it - populates Layer 2.
    ///
    /// Upserts the domain record, then links each named symbol (by name, not ID,
    /// so it works even before the symbol is indexed).
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function with DB side effects)
    pub fn define_domain(&self, args: DefineDomainArgs) -> MemoryResult<()> {
        let conn = self.conn();

        let mut combined_keywords = args.keywords.clone().unwrap_or_default();
        if let Some(aliases) = &args.aliases {
            for alias in aliases {
                if !combined_keywords.contains(alias) {
                    combined_keywords.push(alias.clone());
                }
            }
        }
        let keywords_str = combined_keywords.join(",");

        // Upsert domain
        conn.execute(
            "INSERT INTO domains (name, keywords, auto)
             VALUES (?1, ?2, 0)
             ON CONFLICT(name) DO UPDATE SET keywords = excluded.keywords",
            rusqlite::params![args.name, keywords_str],
        ).map_err(crate::MemoryError::Database)?;

        let domain_id: i64 = conn.query_row(
            "SELECT id FROM domains WHERE name = ?1",
            rusqlite::params![args.name],
            |row| row.get(0),
        ).map_err(crate::MemoryError::Database)?;

        // Link symbols (by name - may not be indexed yet, stored as symbol_name)
        if let Some(symbols) = &args.symbols {
            for sym_name in symbols {
                // Try to find resolved symbol_id
                let sym_id: Option<i64> = conn.query_row(
                    "SELECT id FROM symbols WHERE name = ?1 LIMIT 1",
                    rusqlite::params![sym_name],
                    |row| row.get(0),
                ).optional().map_err(crate::MemoryError::Database)?;

                conn.execute(
                    "INSERT OR IGNORE INTO domain_symbols (domain_id, symbol_id, symbol_name)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![domain_id, sym_id, sym_name],
                ).map_err(crate::MemoryError::Database)?;
            }
        }

        tracing::debug!(domain = %args.name, "fog-memory: domain defined");
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // scratchpad_get() - Layer 5 read
    // ---------------------------------------------------------------------------

    /// Read the current task state for an agent role.
    ///
    /// Returns `None` if no state has been saved for this role.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function)
    pub fn scratchpad_get(&self, role: &str) -> MemoryResult<Option<ScratchpadState>> {
        let conn = self.conn();

        let row: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
            conn.query_row(
                "SELECT agent_role, current_goal, completed_steps, current_errors, blockers, updated_at
                 FROM scratchpad WHERE agent_role = ?1",
                rusqlite::params![role],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                )),
            ).optional().map_err(crate::MemoryError::Database)?;

        Ok(row.map(|(role, goal, steps, errors, blockers, updated_at)| {
            ScratchpadState {
                agent_role: role,
                current_goal: goal,
                completed_steps: parse_json_array(steps.as_deref()),
                current_errors: parse_json_array(errors.as_deref()),
                blockers: parse_json_array(blockers.as_deref()),
                updated_at,
            }
        }))
    }

    // ---------------------------------------------------------------------------
    // scratchpad_update() - Layer 5 write
    // ---------------------------------------------------------------------------

    /// Save or update the task state for an agent role.
    ///
    /// Creates the namespace if it doesn't exist (upsert semantics).
    /// Only fields with `Some(...)` are updated - `None` fields are left unchanged.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function with DB side effect)
    pub fn scratchpad_update(&self, role: &str, state: ScratchpadUpdateArgs) -> MemoryResult<()> {
        let conn = self.conn();

        // Ensure namespace exists
        conn.execute(
            "INSERT OR IGNORE INTO scratchpad (agent_role) VALUES (?1)",
            rusqlite::params![role],
        ).map_err(crate::MemoryError::Database)?;

        // Build dynamic UPDATE for only-provided fields
        let mut set_clauses: Vec<&str> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref goal) = state.current_goal {
            set_clauses.push("current_goal = ?");
            params.push(Box::new(goal.clone()));
        }
        if let Some(ref steps) = state.completed_steps {
            set_clauses.push("completed_steps = ?");
            params.push(Box::new(serde_json::to_string(steps).unwrap_or_default()));
        }
        if let Some(ref errors) = state.current_errors {
            set_clauses.push("current_errors = ?");
            params.push(Box::new(serde_json::to_string(errors).unwrap_or_default()));
        }
        if let Some(ref blockers) = state.blockers {
            set_clauses.push("blockers = ?");
            params.push(Box::new(serde_json::to_string(blockers).unwrap_or_default()));
        }

        if !set_clauses.is_empty() {
            set_clauses.push("updated_at = datetime('now')");
            params.push(Box::new(role.to_string()));

            // Build numbered params SQL (rusqlite doesn't support named params with Box<dyn ToSql>)
            let numbered: String = set_clauses
                .iter()
                .enumerate()
                .map(|(i, clause)| clause.replace("= ?", &format!("= ?{}", i + 1)))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("UPDATE scratchpad SET {numbered} WHERE agent_role = ?{}", params.len());

            conn.execute(
                &sql,
                rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            ).map_err(crate::MemoryError::Database)?;
        }

        tracing::debug!(role, "fog-memory: scratchpad updated");
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // insert_constraint() - Layer 3
    // ---------------------------------------------------------------------------

    /// Insert a constraint (ADR invariant) into the constraints table.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function with DB side effect)
    pub fn insert_constraint(&self, code: &str, severity: &str, statement: &str) -> MemoryResult<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO constraints (code, severity, statement)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(code) DO UPDATE SET severity = excluded.severity, statement = excluded.statement",
            rusqlite::params![code, severity, statement],
        ).map_err(crate::MemoryError::Database)?;
        Ok(conn.last_insert_rowid())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_json_array(s: Option<&str>) -> Vec<String> {
    s.and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::open_test_db;

    #[test]
    fn record_decision_returns_id() {
        let db = open_test_db();
        let id = db.record_decision(RecordDecisionArgs {
            functions: vec!["run_gateway".to_string()],
            reason: "Switched to async runtime".to_string(),
            domain: Some("Gateway".to_string()),
            revert_risk: Some("HIGH".to_string()),
            supersedes_id: None,
        }).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn record_decision_temporal_chaining() {
        let db = open_test_db();
        let id1 = db.record_decision(RecordDecisionArgs {
            functions: vec!["auth".to_string()],
            reason: "Use HMAC".to_string(),
            domain: None,
            revert_risk: None,
            supersedes_id: None,
        }).unwrap();

        let id2 = db.record_decision(RecordDecisionArgs {
            functions: vec!["auth".to_string()],
            reason: "Switch to RS256".to_string(),
            domain: None,
            revert_risk: Some("HIGH".to_string()),
            supersedes_id: Some(id1),
        }).unwrap();

        // Verify old decision is marked historical
        let status: String = db.conn().query_row(
            "SELECT status FROM decisions WHERE id = ?1",
            rusqlite::params![id1],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(status, "historical");
        assert!(id2 > id1);
    }

    #[test]
    fn define_domain_creates_entry() {
        let db = open_test_db();
        db.define_domain(DefineDomainArgs {
            name: "Authentication".to_string(),
            keywords: Some(vec!["auth".to_string(), "jwt".to_string()]),
            symbols: Some(vec!["verify_token".to_string()]),
            constraints: None,
        }).unwrap();

        let count: i64 = db.conn().query_row(
            "SELECT COUNT(*) FROM domains WHERE name = 'Authentication'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);

        // symbol linked by name (not yet indexed)
        let ds_count: i64 = db.conn().query_row(
            "SELECT COUNT(*) FROM domain_symbols WHERE symbol_name = 'verify_token'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(ds_count, 1);
    }

    #[test]
    fn scratchpad_get_empty_returns_none() {
        let db = open_test_db();
        let state = db.scratchpad_get("default").unwrap();
        assert!(state.is_none());
    }

    #[test]
    fn scratchpad_update_then_get() {
        let db = open_test_db();

        db.scratchpad_update("architect", ScratchpadUpdateArgs {
            current_goal: Some("Build fog-memory crate".to_string()),
            completed_steps: Some(vec!["db.rs done".to_string(), "query.rs done".to_string()]),
            current_errors: None,
            blockers: Some(vec!["Waiting for schema alignment".to_string()]),
        }).unwrap();

        let state = db.scratchpad_get("architect").unwrap();
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.agent_role, "architect");
        assert_eq!(state.current_goal.as_deref(), Some("Build fog-memory crate"));
        assert_eq!(state.completed_steps.len(), 2);
        assert_eq!(state.blockers.len(), 1);
    }

    #[test]
    fn scratchpad_multi_role_isolated() {
        let db = open_test_db();

        db.scratchpad_update("coder", ScratchpadUpdateArgs {
            current_goal: Some("Fix bug #42".to_string()),
            ..Default::default()
        }).unwrap();

        db.scratchpad_update("reviewer", ScratchpadUpdateArgs {
            current_goal: Some("Review PR #99".to_string()),
            ..Default::default()
        }).unwrap();

        let coder = db.scratchpad_get("coder").unwrap().unwrap();
        let reviewer = db.scratchpad_get("reviewer").unwrap().unwrap();

        assert_eq!(coder.current_goal.as_deref(), Some("Fix bug #42"));
        assert_eq!(reviewer.current_goal.as_deref(), Some("Review PR #99"));
        // Cross-contamination check
        assert_ne!(coder.current_goal, reviewer.current_goal);
    }
}
