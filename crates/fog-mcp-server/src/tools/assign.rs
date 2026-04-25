//! fog_assign - tag symbols to a business domain.
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
                "name":   { "type": "string", "description": "Domain name (e.g. 'Authentication'). Alias: 'domain'." },
                "domain": { "type": "string", "description": "Alias for 'name'. Either 'name' or 'domain' is required." },
                "symbols": { "type": "array", "items": { "type": "string" } },
                "keywords": { "type": "array", "items": { "type": "string" } },
                "aliases": { "type": "array", "items": { "type": "string" } },
                "constraints": { "type": "array", "items": { "type": "string" } },
                "project": { "type": "string" }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    // E4: Accept "domain" as alias for "name" (AGENTS.md uses "domain")
    let name = args["name"].as_str()
        .or_else(|| args["domain"].as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            return ""
        });
    if name.is_empty() {
        return ToolCallResult::err("fog_assign: 'name' or 'domain' is required");
    }

    let symbols: Vec<String> = args["symbols"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let keywords: Vec<String> = args["keywords"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let aliases: Vec<String> = args["aliases"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let constraints: Vec<String> = args["constraints"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let domain_args = DefineDomainArgs {
        name: name.to_string(),
        symbols: if symbols.is_empty() { None } else { Some(symbols.clone()) },
        keywords: if keywords.is_empty() { None } else { Some(keywords) },
        aliases: if aliases.is_empty() { None } else { Some(aliases) },
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
