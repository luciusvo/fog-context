//! fog-mcp-server/src/router.rs
//!
//! Tool dispatcher - maps tool names to handlers.
//! Returns ToolDef list for `tools/list` and dispatches `tools/call`.
//!
//! PATTERN_DECISION: Level 3 (HOF + Dict Map)
//! Justification: Tool dispatch is pure data-driven branching.
//! No runtime-swappable dependencies → no pattern needed.

use std::path::Path;
use std::sync::{Arc, Mutex};

use fog_memory::MemoryDb;
use serde_json::Value;

use crate::protocol::{ToolCallResult, ToolDef};
use crate::registry::Registry;
use crate::tools;

/// Generate the list of all 15 tools for `tools/list`.
pub fn list_tools() -> Vec<ToolDef> {
    vec![
        // ── Core (8) ────────────────────────────────────────────────────────────
        tools::roots::definition(),
        tools::brief::definition(),
        tools::scan::definition(),
        tools::lookup::definition(),
        tools::outline::definition(),
        tools::inspect::definition(),
        tools::impact::definition(),
        tools::trace::definition(),
        tools::search::definition(),
        // ── Advanced (6) ────────────────────────────────────────────────────────
        tools::gaps::definition(),
        tools::domains::definition(),
        tools::assign::definition(),
        tools::constraints::definition(),
        tools::decisions::definition(),
        tools::import::definition(),
    ]
}

/// Dispatch a `tools/call` request to the appropriate handler.
pub fn dispatch(
    tool_name: &str,
    args: &Value,
    db: &Arc<Mutex<MemoryDb>>,
    project_root: &Path,
    registry: &Registry,
    session_stats: Option<(u64, u64)>,
) -> ToolCallResult {
    // Tools that only need the registry (no DB)
    match tool_name {
        "fog_roots" => return tools::roots::handle(args, registry),
        _ => {}
    }

    // All other tools need the DB
    let db_guard = match db.lock() {
        Ok(g) => g,
        Err(e) => return ToolCallResult::err(format!("DB lock poisoned: {e}")),
    };

    match tool_name {
        "fog_scan"    => tools::scan::handle(args, &db_guard, project_root),
        "fog_import"  => tools::import::handle(args, &db_guard, project_root),

        "fog_brief"       => tools::brief::handle(args, &db_guard, registry, project_root, session_stats),
        "fog_lookup"      => tools::lookup::handle(args, &db_guard, project_root),
        "fog_outline"     => tools::outline::handle(args, &db_guard, project_root),
        "fog_search"      => tools::search::handle(args, project_root),
        "fog_inspect"     => tools::inspect::handle(args, &db_guard, project_root),
        "fog_impact"      => tools::impact::handle(args, &db_guard, project_root),
        "fog_trace"       => tools::trace::handle(args, &db_guard, project_root),
        "fog_gaps"            => tools::gaps::handle(args, &db_guard, project_root),
        "fog_domains"         => tools::domains::handle(args, &db_guard, project_root),
        "fog_assign"          => tools::assign::handle(args, &db_guard),
        "fog_constraints"     => tools::constraints::handle(args, &db_guard, project_root),
        "fog_decisions"       => tools::decisions::handle(args, &db_guard, project_root),
        _ => ToolCallResult::err(format!(
            "Unknown tool: '{tool_name}'. Available: {}",
            list_tools().iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
        )),
    }
}
