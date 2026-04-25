//! fog-memory/query.rs - Read-only intelligence queries
//!
//! Implements 6 read operations mirroring the fog-context TypeScript tools:
//!   search()         → FTS5 BM25 + centrality + time-decay
//!   context_symbol() → 360° symbol view
//!   impact()         → Recursive CTE blast radius
//!   route_map()      → Call tree traversal with token budget
//!   domain_catalog() → List all business domains
//!   knowledge_score()→ Gamification metric (health tool)
//!
//! PATTERN_DECISION: Level 1+2 (Pure Functions + Composition)
//! All functions: (&MemoryDb, args) → Result<T>. Zero side effects.

use crate::{db::MemoryDb, MemoryResult};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: Option<String>,
    pub doc_snippet: Option<String>,
    pub relevance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolContext {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub start_line: i64,
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub callers: Vec<EdgeRef>,
    pub callees: Vec<EdgeRef>,
    pub decisions: Vec<DecisionRef>,
    pub constraints: Vec<ConstraintRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRef {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub start_line: i64,
    pub edge_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRef {
    pub id: i64,
    pub reason: String,
    pub revert_risk: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintRef {
    pub code: String,
    pub severity: String,
    pub statement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactResult {
    pub target: String,
    pub risk: String,
    pub upstream: Vec<ImpactNode>,
    pub downstream: Vec<ImpactNode>,
    pub _agent_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactNode {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub start_line: i64,
    pub depth: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteMapResult {
    pub entry: String,
    pub direction: String,
    pub nodes: Vec<RouteNode>,
    pub truncated: bool,
    pub tokens_estimated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteNode {
    pub depth: i64,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub edge_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainInfo {
    pub id: i64,
    pub name: String,
    pub keywords: Option<String>,
    pub symbol_count: i64,
    pub constraint_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeScore {
    pub total_symbols: i64,
    pub total_files: i64,
    pub total_domains: i64,
    pub total_constraints: i64,
    pub total_decisions: i64,
    pub total_edges: i64,
    pub layer_score: u8,    // 0-100 gamification score
    pub schema_version: String,
    pub _agent_hint: Option<String>,
}

// ---------------------------------------------------------------------------
// search() - FTS5 BM25 + centrality boost + time-decay
// ---------------------------------------------------------------------------

impl MemoryDb {
    /// Full-text search across indexed symbols.
    ///
    /// Uses `symbols_fts` (FTS5 BM25 weighted: name×3, name_tokens×3, sig×2, doc×1).
    /// Returns hits ranked by relevance (BM25 + centrality).
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function)
    pub fn search(&self, query: &str, limit: usize, kind: Option<&str>) -> MemoryResult<Vec<SearchHit>> {
        let conn = self.conn();
        let limit = limit.min(100);
        
        let clean_query = query.trim_end_matches('*');
        let mut fts_terms = vec![];
        
        if query.ends_with('*') {
            fts_terms.push(query.to_string());
        } else {
            fts_terms.push(format!("{}*", clean_query));
        }

        // WP-6 Semantic Synonym Expansion
        if let Ok(mut stmt) = conn.prepare("SELECT keywords FROM domains WHERE name = ?1 OR keywords LIKE ?2 LIMIT 1") {
            let like_query = format!("%{}%", clean_query);
            if let Ok(kws) = stmt.query_row(rusqlite::params![clean_query, like_query], |row| row.get::<_, String>(0)) {
                let mut added = 0;
                for kw in kws.split(',') {
                    if added >= 5 { break; } // Cap at 5 synonyms
                    let kw = kw.trim();
                    if !kw.is_empty() && kw != clean_query {
                        fts_terms.push(format!("{}*", kw));
                        added += 1;
                    }
                }
            }
        }

        let fts_query = fts_terms.join(" OR ");

        let kind_clause = if kind.is_some() { "AND s.kind = ?3" } else { "" };
        let sql = format!(
            "SELECT s.name, s.kind, f.path, s.start_line, s.end_line,
                    s.signature,
                    snippet(symbols_fts, 3, '«', '»', '…', 12) as doc_snippet,
                    bm25(symbols_fts, 3, 3, 2, 1) as bm25_score,
                    COALESCE(s.centrality, 0.0) as centrality
             FROM symbols_fts
             JOIN symbols s ON s.id = symbols_fts.rowid
             JOIN files f ON f.id = s.file_id
             WHERE symbols_fts MATCH ?1 {kind_clause}
             ORDER BY bm25_score ASC
             LIMIT ?2",
        );

        let mut stmt = conn.prepare(&sql).map_err(crate::MemoryError::Database)?;

        let hits = if let Some(k) = kind {
            stmt.query_map(
                rusqlite::params![fts_query, (limit * 3) as i64, k],
                map_search_row,
            )
        } else {
            stmt.query_map(
                rusqlite::params![fts_query, (limit * 3) as i64],
                map_search_row,
            )
        }.map_err(crate::MemoryError::Database)?;

        let mut results: Vec<SearchHit> = hits
            .flatten()
            .collect();

        // Apply centrality boost: relevance = 1/(1 + |bm25|) * (1 + 0.1 * centrality)
        for hit in &mut results {
            let centrality_bonus = 1.0 + 0.1 * hit.relevance; // relevance temporarily holds centrality
            hit.relevance = centrality_bonus; // will be overwritten in map_search_row calculation
        }

        // Sort by relevance desc, truncate to limit
        results.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        Ok(results)
    }

    // ---------------------------------------------------------------------------
    // context_symbol() - 360° view
    // ---------------------------------------------------------------------------

    /// Get full context for a symbol: callers, callees, constraints, decisions.
    ///
    /// PATTERN_DECISION: Level 2 (Composition - builds from 4 sub-queries)
    pub fn context_symbol(&self, name: &str) -> MemoryResult<Option<SymbolContext>> {
        let conn = self.conn();

        // 1. Core symbol record
        let symbol: Option<(i64, String, String, String, i64, Option<String>, Option<String>)> =
            conn.query_row(
                "SELECT s.id, s.name, s.kind, f.path, s.start_line, s.signature, s.doc
                 FROM symbols s JOIN files f ON f.id = s.file_id
                 WHERE s.name = ?1
                 ORDER BY s.id LIMIT 1",
                rusqlite::params![name],
                |row| Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                )),
            ).optional().map_err(crate::MemoryError::Database)?;

        let (sym_id, sym_name, sym_kind, sym_file, start_line, signature, doc) = match symbol {
            Some(s) => s,
            None => return Ok(None),
        };

        // 2. Callers (who calls this symbol)
        let callers = self.fetch_edge_refs(sym_id, "upstream")?;
        // 3. Callees (what this symbol calls)
        let callees = self.fetch_edge_refs(sym_id, "downstream")?;

        // 4. Decisions that mention this symbol
        let mut decisions_stmt = conn.prepare(
            "SELECT id, reason, revert_risk, status, created_at FROM decisions
             WHERE functions LIKE ?1 ORDER BY created_at DESC LIMIT 10",
        ).map_err(crate::MemoryError::Database)?;
        let decisions = decisions_stmt.query_map(
            rusqlite::params![format!("%\"{name}\"%")],
            |row| Ok(DecisionRef {
                id: row.get(0)?,
                reason: row.get(1)?,
                revert_risk: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            }),
        ).map_err(crate::MemoryError::Database)?
        .flatten().collect();

        // 5. Constraints from domains this symbol belongs to
        let mut constraints_stmt = conn.prepare(
            "SELECT DISTINCT c.code, c.severity, c.statement
             FROM constraints c
             JOIN domain_symbols ds ON ds.domain_id = c.domain_id
             WHERE ds.symbol_id = ?1 OR ds.symbol_name = ?2",
        ).map_err(crate::MemoryError::Database)?;
        let constraints = constraints_stmt.query_map(
            rusqlite::params![sym_id, name],
            |row| Ok(ConstraintRef {
                code: row.get(0)?,
                severity: row.get(1)?,
                statement: row.get(2)?,
            }),
        ).map_err(crate::MemoryError::Database)?
        .flatten().collect();

        Ok(Some(SymbolContext {
            name: sym_name,
            kind: sym_kind,
            file: sym_file,
            start_line,
            signature,
            doc,
            callers,
            callees,
            decisions,
            constraints,
        }))
    }

    // #8: Symbol collision helpers

    /// Count how many symbols share the given name (for disambiguation logic).
    pub fn count_symbols_by_name(&self, name: &str) -> MemoryResult<usize> {
        let conn = self.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        ).map_err(crate::MemoryError::Database)?;
        Ok(count as usize)
    }

    /// List (file_path, start_line) pairs for every symbol sharing this name.
    pub fn list_symbols_by_name(&self, name: &str) -> MemoryResult<Vec<(String, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT f.path, s.start_line FROM symbols s
             JOIN files f ON f.id = s.file_id
             WHERE s.name = ?1
             ORDER BY f.path, s.start_line",
        ).map_err(crate::MemoryError::Database)?;
        let pairs = stmt.query_map(rusqlite::params![name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }).map_err(crate::MemoryError::Database)?.flatten().collect();
        Ok(pairs)
    }

    /// Like context_symbol() but filter by file path when name is ambiguous.
    pub fn context_symbol_with_file(
        &self,
        name: &str,
        file_hint: Option<&str>,
    ) -> MemoryResult<Option<SymbolContext>> {
        let conn = self.conn();

        let sql = if file_hint.is_some() {
            "SELECT s.id, s.name, s.kind, f.path, s.start_line, s.signature, s.doc
             FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE s.name = ?1 AND (f.path = ?2 OR f.path LIKE ?3)
             ORDER BY s.id LIMIT 1"
        } else {
            "SELECT s.id, s.name, s.kind, f.path, s.start_line, s.signature, s.doc
             FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE s.name = ?1
             ORDER BY s.id LIMIT 1"
        };

        let file_glob = file_hint.map(|f| format!("%{f}%")).unwrap_or_default();
        let file_val = file_hint.unwrap_or("");

        let symbol: Option<(i64, String, String, String, i64, Option<String>, Option<String>)> =
            conn.query_row(
                sql,
                rusqlite::params![name, file_val, file_glob],
                |row| Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                )),
            ).optional().map_err(crate::MemoryError::Database)?;

        let (sym_id, sym_name, sym_kind, sym_file, start_line, signature, doc) = match symbol {
            Some(s) => s,
            None => return Ok(None),
        };

        let callers = self.fetch_edge_refs(sym_id, "upstream")?;
        let callees = self.fetch_edge_refs(sym_id, "downstream")?;

        let mut decisions_stmt = conn.prepare(
            "SELECT id, reason, revert_risk, status, created_at FROM decisions
             WHERE functions LIKE ?1 ORDER BY created_at DESC LIMIT 10",
        ).map_err(crate::MemoryError::Database)?;
        let decisions = decisions_stmt.query_map(
            rusqlite::params![format!("%\"{name}\"%")],
            |row| Ok(DecisionRef {
                id: row.get(0)?,
                reason: row.get(1)?,
                revert_risk: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            }),
        ).map_err(crate::MemoryError::Database)?.flatten().collect();

        // Constraints + HINT_ entries from Layer 3
        let mut constraints_stmt = conn.prepare(
            "SELECT DISTINCT c.code, c.severity, c.statement
             FROM constraints c
             LEFT JOIN domain_symbols ds ON ds.domain_id = c.domain_id
             WHERE ds.symbol_id = ?1 OR ds.symbol_name = ?2
                OR c.code LIKE 'HINT_%'
             ORDER BY c.code",
        ).map_err(crate::MemoryError::Database)?;
        let constraints = constraints_stmt.query_map(
            rusqlite::params![sym_id, name],
            |row| Ok(ConstraintRef {
                code: row.get(0)?,
                severity: row.get(1)?,
                statement: row.get(2)?,
            }),
        ).map_err(crate::MemoryError::Database)?.flatten().collect();

        Ok(Some(SymbolContext {
            name: sym_name,
            kind: sym_kind,
            file: sym_file,
            start_line,
            signature,
            doc,
            callers,
            callees,
            decisions,
            constraints,
        }))
    }

    fn fetch_edge_refs(&self, sym_id: i64, direction: &str) -> MemoryResult<Vec<EdgeRef>> {
        let sql = if direction == "upstream" {
            // Who calls sym_id
            "SELECT s.name, s.kind, f.path, s.start_line, e.kind
             FROM edges e
             JOIN symbols s ON s.id = e.source_id
             JOIN files f ON f.id = s.file_id
             WHERE e.target_id = ?1
             ORDER BY s.name LIMIT 20"
        } else {
            // What sym_id calls
            "SELECT s.name, s.kind, f.path, s.start_line, e.kind
             FROM edges e
             JOIN symbols s ON s.id = e.target_id
             JOIN files f ON f.id = s.file_id
             WHERE e.source_id = ?1
             ORDER BY s.name LIMIT 20"
        };

        let conn = self.conn();
        let mut stmt = conn.prepare(sql).map_err(crate::MemoryError::Database)?;
        let refs = stmt.query_map(rusqlite::params![sym_id], |row| {
            Ok(EdgeRef {
                name: row.get(0)?,
                kind: row.get(1)?,
                file: row.get(2)?,
                start_line: row.get(3)?,
                edge_kind: row.get(4)?,
            })
        }).map_err(crate::MemoryError::Database)?
        .flatten().collect();
        Ok(refs)
    }

    // ---------------------------------------------------------------------------
    // impact() - Recursive CTE blast radius
    // ---------------------------------------------------------------------------

    /// Compute blast radius: what breaks if symbol `target` changes.
    ///
    /// Returns upstream callers and downstream dependencies up to `depth` hops.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function - recursive CTE, read-only)
    pub fn impact(&self, target: &str, depth: u32, direction: &str) -> MemoryResult<ImpactResult> {
        let conn = self.conn();

        // Resolve target symbol IDs (may have multiple with same name)
        let sym_ids: Vec<i64> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM symbols WHERE name = ?1"
            ).map_err(crate::MemoryError::Database)?;
            let ids: Vec<i64> = stmt.query_map(rusqlite::params![target], |row| row.get(0))
                .map_err(crate::MemoryError::Database)?
                .flatten().collect();
            ids
        };

        let mut upstream: Vec<ImpactNode> = Vec::new();
        let mut downstream: Vec<ImpactNode> = Vec::new();

        for sym_id in &sym_ids {
            // Upstream: who calls this symbol (callers of callers)
            if direction == "upstream" || direction == "both" {
                let rows = self.impact_cte(*sym_id, depth, "upstream")?;
                upstream.extend(rows);
            }
            // Downstream: what this symbol calls
            if direction == "downstream" || direction == "both" {
                let rows = self.impact_cte(*sym_id, depth, "downstream")?;
                downstream.extend(rows);
            }
        }

        // Deduplicate by name
        upstream.dedup_by(|a, b| a.name == b.name);
        downstream.dedup_by(|a, b| a.name == b.name);

        let risk = assess_risk(upstream.len(), downstream.len());

        let hint = if risk == "HIGH" || risk == "CRITICAL" {
            Some(format!(
                "⚠️  {risk} risk. Call record_decision() with revert_risk=\"{risk}\" after modifying '{target}'.",
            ))
        } else {
            None
        };

        Ok(ImpactResult {
            target: target.to_string(),
            risk,
            upstream,
            downstream,
            _agent_hint: hint,
        })
    }

    fn impact_cte(&self, sym_id: i64, max_depth: u32, direction: &str) -> MemoryResult<Vec<ImpactNode>> {
        let (anchor_join, recursive_join) = if direction == "upstream" {
            // Callers: edges.target_id = sym → source_id is the caller
            ("e.target_id = ?1", "e.target_id = ic.id")
        } else {
            // Callees: edges.source_id = sym → target_id is the callee
            ("e.source_id = ?1", "e.source_id = ic.id")
        };

        let walk_col = if direction == "upstream" { "e.source_id" } else { "e.target_id" };

        let sql = format!(
            "WITH RECURSIVE impact_chain(id, name, kind, path, start_line, depth) AS (
                SELECT s.id, s.name, s.kind, f.path, s.start_line, 0
                FROM edges e
                JOIN symbols s ON s.id = {walk_col}
                JOIN files f ON f.id = s.file_id
                WHERE {anchor_join}

                UNION

                SELECT s.id, s.name, s.kind, f.path, s.start_line, ic.depth + 1
                FROM impact_chain ic
                JOIN edges e ON {recursive_join} AND e.kind = 'CALLS'
                JOIN symbols s ON s.id = {walk_col}
                JOIN files f ON f.id = s.file_id
                WHERE ic.depth < ?2
            )
            SELECT DISTINCT id, name, kind, path, start_line, MIN(depth) as depth
            FROM impact_chain
            GROUP BY id
            ORDER BY depth, path",
        );

        let conn = self.conn();
        let mut stmt = conn.prepare(&sql).map_err(crate::MemoryError::Database)?;
        let rows = stmt.query_map(
            rusqlite::params![sym_id, max_depth as i64],
            |row| Ok(ImpactNode {
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                start_line: row.get(4)?,
                depth: row.get(5)?,
            }),
        ).map_err(crate::MemoryError::Database)?
        .flatten().collect();

        Ok(rows)
    }

    // ---------------------------------------------------------------------------
    // route_map() - Call tree traversal
    // ---------------------------------------------------------------------------

    /// Trace the call tree from an entry point symbol.
    ///
    /// `direction`: "down" (callees) | "up" (callers)
    /// `token_budget`: stop adding nodes when estimated token count exceeds budget
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function)
    pub fn route_map(
        &self,
        entry: &str,
        depth: u32,
        direction: &str,
        token_budget: Option<usize>,
    ) -> MemoryResult<RouteMapResult> {
        let budget = token_budget.unwrap_or(usize::MAX);
        let mut nodes: Vec<RouteNode> = Vec::new();
        let mut tokens_used: usize = 0;

        let conn = self.conn();

        // Resolve entry symbol
        let sym_id: Option<i64> = conn.query_row(
            "SELECT id FROM symbols WHERE name = ?1 ORDER BY id LIMIT 1",
            rusqlite::params![entry],
            |row| row.get(0),
        ).optional().map_err(crate::MemoryError::Database)?;

        if sym_id.is_none() {
            return Ok(RouteMapResult {
                entry: entry.to_string(),
                direction: direction.to_string(),
                nodes: Vec::new(),
                truncated: false,
                tokens_estimated: 0,
            });
        }

        let sql = if direction == "down" {
            format!(
                "WITH RECURSIVE call_tree(id, name, kind, path, edge_kind, depth) AS (
                    SELECT s.id, s.name, s.kind, f.path, e.kind, 0
                    FROM edges e
                    JOIN symbols s ON s.id = e.target_id
                    JOIN files f ON f.id = s.file_id
                    WHERE e.source_id = ?1
                    UNION
                    SELECT s.id, s.name, s.kind, f.path, e.kind, ct.depth + 1
                    FROM call_tree ct
                    JOIN edges e ON e.source_id = ct.id
                    JOIN symbols s ON s.id = e.target_id
                    JOIN files f ON f.id = s.file_id
                    WHERE ct.depth < ?2
                )
                SELECT DISTINCT depth, name, kind, path, edge_kind FROM call_tree ORDER BY depth, name",
            )
        } else {
            format!(
                "WITH RECURSIVE call_tree(id, name, kind, path, edge_kind, depth) AS (
                    SELECT s.id, s.name, s.kind, f.path, e.kind, 0
                    FROM edges e
                    JOIN symbols s ON s.id = e.source_id
                    JOIN files f ON f.id = s.file_id
                    WHERE e.target_id = ?1
                    UNION
                    SELECT s.id, s.name, s.kind, f.path, e.kind, ct.depth + 1
                    FROM call_tree ct
                    JOIN edges e ON e.target_id = ct.id
                    JOIN symbols s ON s.id = e.source_id
                    JOIN files f ON f.id = s.file_id
                    WHERE ct.depth < ?2
                )
                SELECT DISTINCT depth, name, kind, path, edge_kind FROM call_tree ORDER BY depth, name",
            )
        };

        let mut stmt = conn.prepare(&sql).map_err(crate::MemoryError::Database)?;
        let rows = stmt.query_map(
            rusqlite::params![sym_id.unwrap(), depth as i64],
            |row| Ok(RouteNode {
                depth: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                edge_kind: row.get(4)?,
            }),
        ).map_err(crate::MemoryError::Database)?;

        let mut truncated = false;
        for row in rows.flatten() {
            let node_tokens = estimate_node_tokens(&row);
            if tokens_used + node_tokens > budget {
                truncated = true;
                break;
            }
            tokens_used += node_tokens;
            nodes.push(row);
        }

        Ok(RouteMapResult {
            entry: entry.to_string(),
            direction: direction.to_string(),
            nodes,
            truncated,
            tokens_estimated: tokens_used,
        })
    }

    // ---------------------------------------------------------------------------
    // domain_catalog() - List all business domains
    // ---------------------------------------------------------------------------

    /// List all registered business domains with symbol and constraint counts.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function)
    pub fn domain_catalog(&self) -> MemoryResult<Vec<DomainInfo>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT d.id, d.name, d.keywords,
                    COUNT(DISTINCT ds.id) as symbol_count,
                    COUNT(DISTINCT dc.constraint_id) as constraint_count
             FROM domains d
             LEFT JOIN domain_symbols ds ON ds.domain_id = d.id
             LEFT JOIN domain_constraints dc ON dc.domain_id = d.id
             GROUP BY d.id
             ORDER BY d.name",
        ).map_err(crate::MemoryError::Database)?;

        let domains = stmt.query_map([], |row| {
            Ok(DomainInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                keywords: row.get(2)?,
                symbol_count: row.get(3)?,
                constraint_count: row.get(4)?,
            })
        }).map_err(crate::MemoryError::Database)?
        .flatten().collect();

        Ok(domains)
    }

    // ---------------------------------------------------------------------------
    // knowledge_score() - Gamification health metric
    // ---------------------------------------------------------------------------

    /// Compute the Knowledge Score - how well-populated are Layers 2-5.
    ///
    /// Score 0-100: penalizes empty domains, constraints, decisions.
    /// Matches the `health()` output from fog-context TypeScript.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function)
    pub fn knowledge_score(&self) -> MemoryResult<KnowledgeScore> {
        let conn = self.conn();

        let count = |sql: &str| -> i64 {
            conn.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap_or(0)
        };

        let total_symbols = count("SELECT COUNT(*) FROM symbols");
        let total_files = count("SELECT COUNT(*) FROM files");
        let total_domains = count("SELECT COUNT(*) FROM domains");
        let total_constraints = count("SELECT COUNT(*) FROM constraints");
        let total_decisions = count("SELECT COUNT(*) FROM decisions");
        let total_edges = count("SELECT COUNT(*) FROM edges");

        let schema_version: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        ).unwrap_or_else(|_| "unknown".to_string());

        // Layer 2-5 score: 0-100
        // Layer 1 (symbols) = baseline (always populated after indexing)
        // Layer 2 (domains): +25 if > 0
        // Layer 3 (constraints): +25 if > 0
        // Layer 4 (decisions): +25 if > 0
        // Layer 5 (scratchpad): checked separately
        let l2 = if total_domains > 0 { 25u8 } else { 0 };
        let l3 = if total_constraints > 0 { 25u8 } else { 0 };
        let l4 = if total_decisions > 0 { 25u8 } else { 0 };
        let l5: u8 = if count("SELECT COUNT(*) FROM scratchpad") > 0 { 25 } else { 0 };
        let layer_score = l2 + l3 + l4 + l5;

        let hint = if layer_score < 50 {
            Some(format!(
                "Knowledge score: {layer_score}/100. Run define_domain, record_decision, and ingest_adrs to improve.",
            ))
        } else {
            None
        };

        Ok(KnowledgeScore {
            total_symbols,
            total_files,
            total_domains,
            total_constraints,
            total_decisions,
            total_edges,
            layer_score,
            schema_version,
            _agent_hint: hint,
        })
    }

    // ---------------------------------------------------------------------------
    // skeleton() - lightweight outline (Phase 4A: queries existing indexed symbols)
    // ---------------------------------------------------------------------------

    /// List symbols in a file or directory, token-efficiently.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function)
    pub fn skeleton(
        &self,
        path: &str,
        max_symbols: usize,
        kind: Option<&str>,
        _include_docs: bool,
    ) -> MemoryResult<Vec<SearchHit>> {
        let conn = self.conn();
        let prefix_glob = format!("{path}%");

        // Use a single sql path with optional kind filter applied post-query to avoid closure type issues
        let base_sql = format!(
            "SELECT s.name, s.kind, f.path, s.start_line, s.end_line,
                    s.signature, s.doc
             FROM symbols s
             JOIN files f ON f.id = s.file_id
             WHERE (f.path = ?1 OR f.path LIKE ?2)
             ORDER BY s.start_line
             LIMIT {}",
            max_symbols * 4, // over-fetch, then filter by kind
        );

        let mut stmt = conn.prepare(&base_sql).map_err(crate::MemoryError::Database)?;
        let hit_map = |row: &rusqlite::Row<'_>| Ok(SearchHit {
            name: row.get(0)?,
            kind: row.get(1)?,
            file: row.get(2)?,
            start_line: row.get(3)?,
            end_line: row.get(4)?,
            signature: row.get(5)?,
            doc_snippet: row.get(6)?,
            relevance: 1.0,
        });

        let all: Vec<SearchHit> = stmt
            .query_map(rusqlite::params![path, prefix_glob], hit_map)
            .map_err(crate::MemoryError::Database)?
            .flatten()
            .collect();

        let hits: Vec<SearchHit> = if let Some(k) = kind {
            all.into_iter().filter(|s| s.kind == k).take(max_symbols).collect()
        } else {
            all.into_iter().take(max_symbols).collect()
        };

        Ok(hits)
    }

    /// #9: Fuzzy skeleton — partial/suffix path match.
    /// Falls back to `WHERE f.path LIKE '%path%'` when exact/prefix match returns nothing.
    pub fn skeleton_fuzzy(
        &self,
        path: &str,
        max_symbols: usize,
        kind: Option<&str>,
        _include_docs: bool,
    ) -> MemoryResult<Vec<SearchHit>> {
        let conn = self.conn();
        let fuzzy_glob = format!("%{path}%");

        let base_sql = format!(
            "SELECT s.name, s.kind, f.path, s.start_line, s.end_line,
                    s.signature, s.doc
             FROM symbols s
             JOIN files f ON f.id = s.file_id
             WHERE f.path LIKE ?1
             ORDER BY LENGTH(f.path), s.start_line
             LIMIT {}",
            max_symbols * 4,
        );

        let mut stmt = conn.prepare(&base_sql).map_err(crate::MemoryError::Database)?;
        let hit_map = |row: &rusqlite::Row<'_>| Ok(SearchHit {
            name: row.get(0)?,
            kind: row.get(1)?,
            file: row.get(2)?,
            start_line: row.get(3)?,
            end_line: row.get(4)?,
            signature: row.get(5)?,
            doc_snippet: row.get(6)?,
            relevance: 0.8, // lower relevance: fuzzy match
        });

        let all: Vec<SearchHit> = stmt
            .query_map(rusqlite::params![fuzzy_glob], hit_map)
            .map_err(crate::MemoryError::Database)?
            .flatten()
            .collect();

        let hits: Vec<SearchHit> = if let Some(k) = kind {
            all.into_iter().filter(|s| s.kind == k).take(max_symbols).collect()
        } else {
            all.into_iter().take(max_symbols).collect()
        };

        Ok(hits)
    }

    // ---------------------------------------------------------------------------
    // query_domain() - get full domain detail (symbols + constraints + decisions)
    // ---------------------------------------------------------------------------

    /// Query a specific business domain by name.
    ///
    /// PATTERN_DECISION: Level 1 (Pure Function)
    pub fn query_domain(&self, domain: &str) -> MemoryResult<Option<DomainDetail>> {
        let conn = self.conn();

        // Find domain
        let domain_row: Option<(i64, String, Option<String>)> = conn.query_row(
            "SELECT id, name, keywords FROM domains WHERE name = ?1",
            rusqlite::params![domain],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional().map_err(crate::MemoryError::Database)?;

        let Some((domain_id, domain_name, keywords)) = domain_row else {
            return Ok(None);
        };

        // Get linked symbols
        let mut sym_stmt = conn.prepare(
            "SELECT s.name, s.kind, f.path, s.start_line
             FROM domain_symbols ds
             JOIN symbols s ON s.id = ds.symbol_id
             JOIN files f ON f.id = s.file_id
             WHERE ds.domain_id = ?1
             ORDER BY s.name",
        ).map_err(crate::MemoryError::Database)?;
        let symbols: Vec<EdgeRef> = sym_stmt.query_map(
            rusqlite::params![domain_id],
            |row| Ok(EdgeRef {
                name: row.get(0)?,
                kind: row.get(1)?,
                file: row.get(2)?,
                start_line: row.get(3)?,
                edge_kind: "domain_member".to_string(),
            }),
        ).map_err(crate::MemoryError::Database)?.flatten().collect();

        // Get constraints
        let mut con_stmt = conn.prepare(
            "SELECT c.code, c.severity, c.statement
             FROM domain_constraints dc
             JOIN constraints c ON c.id = dc.constraint_id
             WHERE dc.domain_id = ?1",
        ).map_err(crate::MemoryError::Database)?;
        let constraints: Vec<ConstraintRef> = con_stmt.query_map(
            rusqlite::params![domain_id],
            |row| Ok(ConstraintRef {
                code: row.get(0)?,
                severity: row.get(1)?,
                statement: row.get(2)?,
            }),
        ).map_err(crate::MemoryError::Database)?.flatten().collect();

        // Get decisions
        let mut dec_stmt = conn.prepare(
            "SELECT d.id, d.reason, d.revert_risk, d.status, d.created_at
             FROM decisions d
             WHERE d.domain = ?1
             ORDER BY d.created_at DESC
             LIMIT 20",
        ).map_err(crate::MemoryError::Database)?;
        let decisions: Vec<DecisionRef> = dec_stmt.query_map(
            rusqlite::params![domain_name],
            |row| Ok(DecisionRef {
                id: row.get(0)?,
                reason: row.get(1)?,
                revert_risk: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            }),
        ).map_err(crate::MemoryError::Database)?.flatten().collect();

        Ok(Some(DomainDetail {
            name: domain_name,
            keywords,
            symbols,
            constraints,
            decisions,
        }))
    }

    // ---------------------------------------------------------------------------
    // graph_query() - safe templated graph analysis
    // ---------------------------------------------------------------------------

    /// Run a pre-defined graph analysis template.
    ///
    /// Templates: find_cycles, find_orphans, find_shared_callers, find_communities, find_path
    ///
    /// PATTERN_DECISION: Level 3 (HOF + dispatch map - template_name → query fn)
    pub fn graph_query(&self, template: &str, params: &serde_json::Value) -> MemoryResult<Vec<serde_json::Value>> {
        use serde_json::json;
        let conn = self.conn();

        match template {
            "find_orphans" => {
                let kind = params["kind"].as_str().unwrap_or("function");
                let limit = params["limit"].as_u64().unwrap_or(20) as i64;
                let mut stmt = conn.prepare(
                    "SELECT s.name, s.kind, f.path, s.start_line
                     FROM symbols s
                     JOIN files f ON f.id = s.file_id
                     WHERE s.kind = ?1
                     AND s.id NOT IN (SELECT DISTINCT target_id FROM edges WHERE kind='CALLS')
                     AND s.id NOT IN (SELECT DISTINCT source_id FROM edges WHERE kind='CALLS')
                     ORDER BY s.name LIMIT ?2"
                ).map_err(crate::MemoryError::Database)?;
                let results = stmt.query_map(rusqlite::params![kind, limit], |row| {
                    Ok(json!({ "name": row.get::<_,String>(0)?, "kind": row.get::<_,String>(1)?,
                                "file": row.get::<_,String>(2)?, "line": row.get::<_,i64>(3)? }))
                }).map_err(crate::MemoryError::Database)?.flatten().collect();
                Ok(results)
            }
            "find_cycles" => {
                // Simplified cycle detection: symbols that call themselves (direct cycle)
                let mut stmt = conn.prepare(
                    "SELECT s.name, s.kind, f.path
                     FROM edges e
                     JOIN symbols s ON s.id = e.source_id
                     JOIN files f ON f.id = s.file_id
                     WHERE e.source_id = e.target_id AND e.kind = 'CALLS'
                     LIMIT 50"
                ).map_err(crate::MemoryError::Database)?;
                let results = stmt.query_map([], |row| {
                    Ok(json!({ "name": row.get::<_,String>(0)?, "kind": row.get::<_,String>(1)?,
                                "file": row.get::<_,String>(2)?, "type": "self_recursion" }))
                }).map_err(crate::MemoryError::Database)?.flatten().collect();
                Ok(results)
            }
            "find_path" => {
                let from = params["from"].as_str().unwrap_or("");
                let to   = params["to"].as_str().unwrap_or("");
                if from.is_empty() || to.is_empty() {
                    return Err(crate::MemoryError::Database(
                        rusqlite::Error::InvalidQuery
                    ));
                }
                let exists: bool = conn.query_row(
                    "SELECT COUNT(*) > 0 FROM edges e \
                     JOIN symbols s1 ON s1.id = e.source_id AND s1.name = ?1 \
                     JOIN symbols s2 ON s2.id = e.target_id AND s2.name = ?2",
                    rusqlite::params![from, to],
                    |row| row.get(0),
                ).unwrap_or(false);
                if exists {
                    Ok(vec![json!({ "from": from, "to": to, "direct": true, "path": [from, to] })])
                } else {
                    Ok(vec![])
                }
            }
            "find_shared_callers" => {
                let a = params["a"].as_str().unwrap_or("");
                let b = params["b"].as_str().unwrap_or("");
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT s.name, s.kind, f.path
                     FROM edges e1
                     JOIN symbols s ON s.id = e1.source_id
                     JOIN files f ON f.id = s.file_id
                     JOIN symbols target1 ON target1.id = e1.target_id AND target1.name = ?1
                     WHERE e1.source_id IN (
                         SELECT e2.source_id FROM edges e2
                         JOIN symbols target2 ON target2.id = e2.target_id AND target2.name = ?2
                     ) LIMIT 20"
                ).map_err(crate::MemoryError::Database)?;
                let results = stmt.query_map(rusqlite::params![a, b], |row| {
                    Ok(json!({ "caller": row.get::<_,String>(0)?, "kind": row.get::<_,String>(1)?,
                                "file": row.get::<_,String>(2)? }))
                }).map_err(crate::MemoryError::Database)?.flatten().collect();
                Ok(results)
            }
            other => Err(crate::MemoryError::Database(
                rusqlite::Error::InvalidQuery
            )),
        }
    }
}

/// Full domain detail (for query_domain).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DomainDetail {
    pub name: String,
    pub keywords: Option<String>,
    pub symbols: Vec<EdgeRef>,
    pub constraints: Vec<ConstraintRef>,
    pub decisions: Vec<DecisionRef>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn map_search_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchHit> {
    let bm25: f64 = row.get(7)?;
    let centrality: f64 = row.get(8)?;
    // Normalize BM25 (negative) to relevance [0,1] + centrality boost
    let relevance = 1.0 / (1.0 + bm25.abs()) * (1.0 + 0.1 * centrality);
    Ok(SearchHit {
        name: row.get(0)?,
        kind: row.get(1)?,
        file: row.get(2)?,
        start_line: row.get(3)?,
        end_line: row.get(4)?,
        signature: row.get(5)?,
        doc_snippet: row.get(6)?,
        relevance,
    })
}

fn assess_risk(upstream_count: usize, downstream_count: usize) -> String {
    let total = upstream_count + downstream_count;
    match total {
        0..=2 => "LOW".to_string(),
        3..=10 => "MEDIUM".to_string(),
        11..=30 => "HIGH".to_string(),
        _ => "CRITICAL".to_string(),
    }
}

fn estimate_node_tokens(node: &RouteNode) -> usize {
    // Rough token estimate: ~4 chars per token, plus structure overhead
    (node.name.len() + node.kind.len() + node.file.len()) / 4 + 8
}

// ---------------------------------------------------------------------------
// MemoryEngine trait impl (wires into lib.rs trait)
// ---------------------------------------------------------------------------

use crate::MemoryEngine;
use crate::write;

impl MemoryEngine for MemoryDb {
    fn search(&self, query: &str, limit: usize, kind: Option<&str>) -> MemoryResult<Vec<SearchHit>> {
        self.search(query, limit, kind)
    }
    fn context_symbol(&self, name: &str) -> MemoryResult<Option<SymbolContext>> {
        self.context_symbol(name)
    }
    fn impact(&self, target: &str, depth: u32, direction: &str) -> MemoryResult<ImpactResult> {
        self.impact(target, depth, direction)
    }
    fn route_map(&self, entry: &str, depth: u32, direction: &str, token_budget: Option<usize>) -> MemoryResult<RouteMapResult> {
        self.route_map(entry, depth, direction, token_budget)
    }
    fn domain_catalog(&self) -> MemoryResult<Vec<DomainInfo>> {
        self.domain_catalog()
    }
    fn knowledge_score(&self) -> MemoryResult<KnowledgeScore> {
        self.knowledge_score()
    }
    fn record_decision(&self, args: write::RecordDecisionArgs) -> MemoryResult<i64> {
        self.record_decision(args)
    }
    fn define_domain(&self, args: write::DefineDomainArgs) -> MemoryResult<()> {
        self.define_domain(args)
    }
    fn scratchpad_get(&self, role: &str) -> MemoryResult<Option<write::ScratchpadState>> {
        self.scratchpad_get(role)
    }
    fn scratchpad_update(&self, role: &str, state: write::ScratchpadUpdateArgs) -> MemoryResult<()> {
        self.scratchpad_update(role, state)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::open_test_db;

    fn seed_symbols(db: &MemoryDb) -> (i64, i64) {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO files (path, lang) VALUES ('src/gateway.rs', 'rust')",
            [],
        ).unwrap();
        let file_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO symbols (file_id, name, kind, start_line, end_line, signature, doc, name_tokens)
             VALUES (?1, 'run_gateway', 'fn', 10, 50, 'pub fn run_gateway()', 'Main gateway loop', 'run gateway')",
            rusqlite::params![file_id],
        ).unwrap();
        let sym1 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO symbols (file_id, name, kind, start_line, end_line, signature, name_tokens)
             VALUES (?1, 'check_quota', 'fn', 55, 70, 'fn check_quota() -> bool', 'check quota')",
            rusqlite::params![file_id],
        ).unwrap();
        let sym2 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO edges (source_id, target_id, kind) VALUES (?1, ?2, 'CALLS')",
            rusqlite::params![sym1, sym2],
        ).unwrap();

        (sym1, sym2)
    }

    #[test]
    fn search_finds_symbol() {
        let db = open_test_db();
        seed_symbols(&db);
        let hits = db.search("gateway", 10, None).unwrap();
        assert!(!hits.is_empty(), "should find run_gateway");
        assert!(hits.iter().any(|h| h.name.contains("gateway")));
    }

    #[test]
    fn context_symbol_returns_edges() {
        let db = open_test_db();
        seed_symbols(&db);
        let ctx = db.context_symbol("run_gateway").unwrap();
        assert!(ctx.is_some(), "should find run_gateway");
        let ctx = ctx.unwrap();
        assert_eq!(ctx.kind, "fn");
        assert!(!ctx.callees.is_empty(), "run_gateway should call check_quota");
    }

    #[test]
    fn knowledge_score_empty_db() {
        let db = open_test_db();
        let score = db.knowledge_score().unwrap();
        assert_eq!(score.layer_score, 0, "empty DB should score 0");
        assert_eq!(score.schema_version, "0.4.0");
    }

    #[test]
    fn domain_catalog_empty() {
        let db = open_test_db();
        let domains = db.domain_catalog().unwrap();
        assert!(domains.is_empty());
    }
}
