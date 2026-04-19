//! fog_add_constraint - Push-based Layer 3 constraint injection.
//!
//! Allows AI agents to directly insert architecture constraints into the DB
//! without needing to create and scan ADR files. The agent reads any source
//! (issue, PR, Notion doc, YAML) and calls this tool with the extracted rule.
//!
//! Complements fog_constraints (file-scan based) - both write to the same table.

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_add_constraint",
        description: "Directly insert an architecture constraint into Layer 3 without needing ADR files. \
            AI agents can read any source (docs, issues, PRs) and push rules into fog-context. \
            Complements fog_constraints (file-scan based).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "Machine-readable constraint ID (e.g. 'NO_DIRECT_DB', 'NO_GLOBAL_STATE')."
                },
                "severity": {
                    "type": "string",
                    "enum": ["ERROR", "WARNING", "INFO"],
                    "description": "Severity level. Default: ERROR."
                },
                "statement": {
                    "type": "string",
                    "description": "Human-readable rule (e.g. 'HTTP handlers must not call the database directly')."
                },
                "project": { "type": "string" }
            },
            "required": ["code", "statement"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let code = match args["code"].as_str().filter(|s| !s.is_empty()) {
        Some(c) => c,
        None => return ToolCallResult::err("fog_add_constraint: 'code' is required"),
    };
    let statement = match args["statement"].as_str().filter(|s| !s.is_empty()) {
        Some(s) => s,
        None => return ToolCallResult::err("fog_add_constraint: 'statement' is required"),
    };
    let severity = args["severity"].as_str().unwrap_or("ERROR");

    // Validate severity
    if !matches!(severity, "ERROR" | "WARNING" | "INFO") {
        return ToolCallResult::err("fog_add_constraint: severity must be ERROR, WARNING, or INFO");
    }

    match db.insert_constraint(code, severity, statement) {
        Ok(_) => ToolCallResult::ok(format!(
            "✅ Constraint added to Layer 3:\n\
             - **Code:** `{code}`\n\
             - **Severity:** {severity}\n\
             - **Rule:** {statement}\n\n\
             Verify with fog_brief({{}}) - check Constraints (L3) count."
        )),
        Err(e) => ToolCallResult::err(format!("fog_add_constraint error: {e}")),
    }
}
