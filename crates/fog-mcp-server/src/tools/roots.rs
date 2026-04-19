//! fog_roots - list registered workspaces/projects.
//!
//! Replaces: list_repos
//! Returns all projects in the global registry with their indexing status.

use serde_json::{json, Value};
use crate::protocol::{ToolCallResult, ToolDef};
use crate::registry::Registry;

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_roots",
        description: "List all registered projects/workspaces in the fog-context registry. \
            Returns project paths, names, symbol counts, and last-indexed timestamps. \
            CALL THIS FIRST when starting a new session to discover available projects.",
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

pub fn handle(_args: &Value, registry: &Registry) -> ToolCallResult {
    let repos = registry.list();
    if repos.is_empty() {
        return ToolCallResult::ok(
            "No projects registered.\n\
             Run `fog_scan({ \"project\": \"/path/to/project\" })` to index a new project."
        );
    }

    let active = registry.active_project.as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut lines = vec![
        format!("# Registered Projects (fog-context v{})\n", env!("CARGO_PKG_VERSION")),
        "| Status | Name | Symbols | Last Indexed | fog_id |".to_string(),
        "|--------|------|---------|--------------|--------|".to_string(),
    ];

    let mut first = true;
    for repo in repos {
        let is_default = repo.path == active || (active.is_empty() && first);
        first = false;
        let marker = if is_default { "🎯 default" } else { "   ·  " };
        let fog_id_short = repo.fog_id.as_deref()
            .map(|id| format!("{}…", &id[..id.len().min(8)]))
            .unwrap_or_else(|| "-".to_string());
        lines.push(format!(
            "| {} | **{}** | {} | {} | `{}` |",
            marker,
            repo.name,
            repo.symbol_count.unwrap_or(0),
            repo.last_indexed.as_deref().unwrap_or("never"),
            fog_id_short,
        ));
    }

    lines.push(String::new());
    lines.push("**Multi-project tip:** Pass `\"project\": \"<name>\"` to any tool to route to a specific project.".to_string());
    lines.push("**Fuzzy match:** short names work (\"fog\" matches \"fog-context\") unless ambiguous.".to_string());

    ToolCallResult::ok(lines.join("\n"))
}
