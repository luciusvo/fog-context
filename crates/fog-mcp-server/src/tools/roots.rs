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
            "No projects registered. Run `fog-mcp-server --project /path/to/project` \
             or call fog_scan to index a new project."
        );
    }

    let mut lines = vec!["# Registered Projects\n".to_string()];
    for repo in repos {
        lines.push(format!(
            "## {}\n- Path: {}\n- Symbols: {}\n- Last indexed: {}\n",
            repo.name,
            repo.path,
            repo.symbol_count.unwrap_or(0),
            repo.last_indexed.as_deref().unwrap_or("never"),
        ));
    }

    ToolCallResult::ok(lines.join("\n"))
}
