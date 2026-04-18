//! fog_assign — tag symbols to a business domain.
//! Replaces: define_domain

use fog_memory::{MemoryDb, write::DefineDomainArgs};
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_assign",
        description: "Create or update a business domain and link symbols to it. \
            Builds the Layer 2 (Business) knowledge map. \
            After this, fog_domains will return the full picture for that domain.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Domain name (e.g. 'Authentication')." },
                "symbols": { "type": "array", "items": { "type": "string" } },
                "keywords": { "type": "array", "items": { "type": "string" } },
                "constraints": { "type": "array", "items": { "type": "string" } },
                "project": { "type": "string" }
            },
            "required": ["name"]
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    let name = match args["name"].as_str() {
        Some(n) if !n.is_empty() => n,
        _ => return ToolCallResult::err("fog_assign: 'name' is required"),
    };

    let symbols: Vec<String> = args["symbols"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let keywords: Vec<String> = args["keywords"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let constraints: Vec<String> = args["constraints"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let domain_args = DefineDomainArgs {
        name: name.to_string(),
        symbols: if symbols.is_empty() { None } else { Some(symbols.clone()) },
        keywords: if keywords.is_empty() { None } else { Some(keywords) },
        constraints: if constraints.is_empty() { None } else { Some(constraints) },
    };

    match db.define_domain(domain_args) {
        Ok(()) => ToolCallResult::ok(format!(
            "✅ Domain '{name}' saved. {} symbols linked.\nVerify: fog_domains(domain='{name}')",
            symbols.len(),
        )),
        Err(e) => ToolCallResult::err(format!("fog_assign error: {e}")),
    }
}
