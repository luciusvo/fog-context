//! fog_domains — business domain catalog + query (merged tool).
//! Replaces: domain_catalog + query_domain

use fog_memory::MemoryDb;
use serde_json::{json, Value};
use crate::protocol::ToolCallResult;
pub use crate::protocol::ToolDef;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_domains",
        description: "Business domain catalog (no args) or domain detail query (with 'domain' arg).\n\
            Without 'domain': lists all known business domains with keyword aliases.\n\
            With 'domain': returns all functions, constraints, and decision history for that domain.\n\
            USE THIS to map natural language to code concepts before searching.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "domain": { "type": "string", "description": "Domain name to query. Omit to list all." },
                "project": { "type": "string" }
            }
        }),
    }
}

pub fn handle(args: &Value, db: &MemoryDb) -> ToolCallResult {
    if let Some(domain) = args["domain"].as_str() {
        match db.query_domain(domain) {
            Ok(None) => ToolCallResult::ok(format!(
                "Domain '{domain}' not found. Call fog_domains (no args) to see all domains."
            )),
            Ok(Some(info)) => {
                let mut lines = vec![format!("# Domain: {}\n", info.name)];
                if let Some(kw) = &info.keywords {
                    lines.push(format!("**Keywords:** {kw}"));
                }
                if !info.symbols.is_empty() {
                    lines.push(format!("\n## Symbols ({} functions)", info.symbols.len()));
                    for s in &info.symbols {
                        lines.push(format!("- `{}` [{}] — {}", s.name, s.kind, s.file));
                    }
                }
                if !info.constraints.is_empty() {
                    lines.push("\n## Constraints".to_string());
                    for c in &info.constraints {
                        lines.push(format!("- **{}** [{}]: {}", c.code, c.severity, c.statement));
                    }
                }
                if !info.decisions.is_empty() {
                    lines.push("\n## Decisions".to_string());
                    for d in &info.decisions {
                        lines.push(format!("- [{}] {} (risk: {})", d.created_at, d.reason, d.revert_risk));
                    }
                }
                ToolCallResult::ok(lines.join("\n"))
            }
            Err(e) => ToolCallResult::err(format!("fog_domains error: {e}")),
        }
    } else {
        match db.domain_catalog() {
            Ok(domains) => {
                if domains.is_empty() {
                    return ToolCallResult::ok(
                        "No business domains defined yet. Use fog_assign to tag symbols to domains."
                    );
                }
                let mut lines = vec![format!("# Business Domains ({} total)\n", domains.len())];
                for d in &domains {
                    lines.push(format!("## {}\n**Keywords:** {}\n**Symbols:** {} | **Constraints:** {}",
                        d.name,
                        d.keywords.as_deref().unwrap_or("(none)"),
                        d.symbol_count, d.constraint_count,
                    ));
                }
                ToolCallResult::ok(lines.join("\n\n"))
            }
            Err(e) => ToolCallResult::err(format!("fog_domains catalog error: {e}")),
        }
    }
}
