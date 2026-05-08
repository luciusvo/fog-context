//! DSL Checker Engine
//! Evaluates Layer 3 constraints against Layer 1/2 data.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Violation {
    pub constraint_code: String,
    pub severity: String,
    pub statement: String,
    pub source_symbol: String,
    pub target_symbol: String,
    pub source_file: String,
    pub target_file: String,
}

/// Evaluates 'forbidden_edge' constraints.
/// A 'forbidden_edge' constraint specifies that symbols in a particular domain
/// must not call target symbols that match the `rule_config` pattern.
pub fn validate_edges(conn: &Connection) -> Result<Vec<Violation>, rusqlite::Error> {
    let sql = r#"
        SELECT
            c.code,
            c.severity,
            c.statement,
            s_src.name as source_symbol,
            s_tgt.name as target_symbol,
            f_src.path as source_file,
            f_tgt.path as target_file
        FROM constraints c
        JOIN domain_symbols ds_src ON ds_src.domain_id = c.domain_id
        JOIN symbols s_src ON s_src.name = ds_src.symbol_name
        JOIN edges e ON e.source_id = s_src.id
        JOIN symbols s_tgt ON s_tgt.id = e.target_id
        JOIN files f_src ON f_src.id = s_src.file_id
        JOIN files f_tgt ON f_tgt.id = s_tgt.file_id
        JOIN symbol_tags st_tgt ON st_tgt.symbol_name = s_tgt.name
        WHERE c.rule_type = 'forbidden_edge'
          AND c.rule_config IS NOT NULL
          AND (st_tgt.tag_value = json_extract(c.rule_config, '$.to') OR st_tgt.tag_type = json_extract(c.rule_config, '$.to'))
    "#;

    let mut stmt = conn.prepare(sql)?;
    let violations = stmt.query_map([], |row| {
        Ok(Violation {
            constraint_code: row.get(0)?,
            severity: row.get(1)?,
            statement: row.get(2)?,
            source_symbol: row.get(3)?,
            target_symbol: row.get(4)?,
            source_file: row.get(5)?,
            target_file: row.get(6)?,
        })
    })?.flatten().collect();

    Ok(violations)
}
