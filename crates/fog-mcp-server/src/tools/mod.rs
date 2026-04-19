//! fog-mcp-server/src/tools/mod.rs
//!
//! 15 fog-context v0.5.5 tool handlers.
//! Each tool delegates its DB work to fog-memory - NO direct SQL here.
//!
//! ## Tool Inventory
//!
//! | Name              | Replaces         | Group    |
//! |:------------------|:-----------------|:---------|
//! | fog_roots         | list_repos       | Core     |
//! | fog_scan          | index            | Core     |
//! | fog_lookup        | search           | Core     |
//! | fog_outline       | skeleton         | Core     |
//! | fog_inspect       | context          | Core     |
//! | fog_impact        | impact           | Core     |
//! | fog_trace         | route_map        | Core     |
//! | fog_brief         | health           | Core     |
//! | fog_gaps          | graph_query      | Advanced |
//! | fog_domains       | domain_catalog + query_domain | Advanced |
//! | fog_assign        | define_domain    | Advanced |
//! | fog_constraints   | ingest_adrs      | Advanced |
//! | fog_decisions     | record_decision  | Advanced |
//! | fog_import        | NEW: BRV+GitNexus L2/L3/L4 pump | Advanced |

pub mod add_constraint;
pub mod assign;
pub mod brief;
pub mod decisions;
pub mod domains;
pub mod gaps;
pub mod impact;
pub mod import;
pub mod inspect;
pub mod lookup;
pub mod outline;
pub mod roots;
pub mod scan;
pub mod trace;
pub mod constraints;

