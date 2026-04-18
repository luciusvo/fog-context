//! fog_decisions — log WHY a code change was made.
//! Replaces: record_decision

use fog_memory::{MemoryDb, write::RecordDecisionArgs};
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_decisions",
        description: "Record WHY you changed code. Call this AFTER completing changes. \
            Builds institutional memory that future AI sessions read via fog_inspect. \
            MANDATORY after every significant architectural or behavioral change.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "functions": { "type": "array", "items": { "type": "string" }, "description": "Function names changed." },
                "reason": { "type": "string", "description": "Why this change was made." },
                "domain": { "type": "string", "description": "Business domain (optional)." },
                "revert_risk": { "type": "string", "enum": ["LOW", "MEDIUM", "HIGH"], "default": "LOW" },
                "project": { "type": "string" }
            },
            "required": ["functions", "reason"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let functions: Vec<String> = match args["functions"].as_array() {
        Some(a) if !a.is_empty() => a.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => return ToolCallResult::err("fog_decisions: 'functions' is required"),
    };
    let reason = match args["reason"].as_str() {
        Some(r) if !r.is_empty() => r,
        _ => return ToolCallResult::err("fog_decisions: 'reason' is required"),
    };

    let decision_args = RecordDecisionArgs {
        functions: functions.clone(),
        reason: reason.to_string(),
        domain: args["domain"].as_str().map(String::from),
        revert_risk: args["revert_risk"].as_str().map(String::from),
        supersedes_id: None,
    };

    match db.record_decision(decision_args) {
        Ok(id) => ToolCallResult::ok(format!(
            "✅ Decision recorded (id={id}).\nFunctions: {}\nReason: {reason}\n\
            Visible in fog_inspect for all listed functions.",
            functions.join(", "),
        )),
        Err(e) => ToolCallResult::err(format!("fog_decisions error: {e}")),
    }
}
