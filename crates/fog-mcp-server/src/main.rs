//! fog-mcp-server — Dual-mode binary: MCP server + CLI tool.
//!
//! fog-context v0.5.0 — Rust rewrite of the TypeScript fog-context-repo.
//!
//! ## MCP Mode (Default — for AI Agents)
//! JSON-RPC 2.0 over stdio. Connect from Cursor, Cline, Claude Desktop, Zed.
//!
//! ## CLI Mode (for humans / CI pipelines)
//!   fog-mcp-server index [--full]                # Index project and exit
//!   fog-mcp-server stats                         # Print index statistics
//!   fog-mcp-server export [--format xml|md|json] # Export snapshot to stdout
//!   fog-mcp-server --list-tools                  # List available MCP tools
//!
//! ## Claude Desktop config example:
//! ```json
//! {
//!   "mcpServers": {
//!     "fog-context": {
//!       "command": "/path/to/fog-mcp-server",
//!       "args": ["--project", "/path/to/your/project"]
//!     }
//!   }
//! }
//! ```

mod indexer;
mod protocol;
mod registry;
mod router;
mod tools;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fog_memory::{MemoryDb, open_from_project};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use protocol::{McpRequest, ToolCallResult, err_response, ok_response, ERR_METHOD};
use registry::Registry;

// ---------------------------------------------------------------------------
// CLI args & subcommand dispatch
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Cmd {
    /// Default: Launch MCP stdio server
    Serve,
    /// `fog-mcp-server index [--full]`
    Index { full: bool },
    /// `fog-mcp-server stats`
    Stats,
    /// `fog-mcp-server export [--format xml|md|json]`
    Export { format: String },
    /// `fog-mcp-server --list-tools`
    ListTools,
}

struct Args {
    project: Option<PathBuf>,
    cmd: Cmd,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut project: Option<PathBuf> = None;
    let mut cmd = Cmd::Serve;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--project" | "-p" => {
                i += 1;
                if let Some(p) = raw.get(i) { project = Some(PathBuf::from(p)); }
            }
            "--list-tools" => cmd = Cmd::ListTools,
            // --- Subcommands ---
            "index" => {
                let full = raw.get(i + 1).map(|s| s == "--full").unwrap_or(false);
                if full { i += 1; }
                cmd = Cmd::Index { full };
            }
            "stats" => cmd = Cmd::Stats,
            "export" => {
                let format = if raw.get(i + 1).map(|s| s == "--format").unwrap_or(false) {
                    i += 2;
                    raw.get(i).cloned().unwrap_or_else(|| "json".into())
                } else {
                    "json".into()
                };
                cmd = Cmd::Export { format };
            }
            _ => {}
        }
        i += 1;
    }
    Args { project, cmd }
}

// ---------------------------------------------------------------------------
// MCP request handlers
// ---------------------------------------------------------------------------

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "fog-context",
            "version": env!("CARGO_PKG_VERSION"),
        }
    })
}

fn handle_list_tools() -> Value {
    let tools = router::list_tools();
    let tool_list: Vec<Value> = tools.iter().map(|t| json!({
        "name": t.name,
        "description": t.description,
        "inputSchema": t.input_schema,
    })).collect();
    json!({ "tools": tool_list })
}

fn handle_tools_call(
    params: &Value,
    db: &Arc<Mutex<MemoryDb>>,
    project_root: &std::path::Path,
    registry: &Registry,
) -> Value {
    let tool_name = match params["name"].as_str() {
        Some(n) => n,
        None => {
            let result = ToolCallResult::err("tools/call: 'name' field is required");
            return json!({ "content": result.content, "isError": result.is_error });
        }
    };

    let args = &params["arguments"];
    let result = router::dispatch(tool_name, args, db, project_root, registry);
    json!({
        "content": result.content,
        "isError": result.is_error,
    })
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let args = parse_args();

    // Resolve project root
    let project_root = args.project.unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    // ===========================================================
    // CLI sub-command dispatch (non-interactive, exits when done)
    // ===========================================================
    match args.cmd {
        Cmd::ListTools => {
            let tools = router::list_tools();
            println!("fog-context v{} — {} tools available:", env!("CARGO_PKG_VERSION"), tools.len());
            for t in &tools {
                println!("  {} — {}", t.name, t.description.lines().next().unwrap_or(""));
            }
            return;
        }

        Cmd::Index { full } => {
            eprintln!("[fog] Indexing: {} (full={})", project_root.display(), full);
            let db = open_from_project(&project_root).unwrap_or_else(|e| {
                eprintln!("[fog] DB error: {e}"); std::process::exit(1);
            });
            let result = crate::indexer::run_scan(&project_root, &db, full);
            if result.is_error {
                eprintln!("[fog] Index failed.");
                std::process::exit(1);
            }
            let text = result.content.first().map(|c| c.text.as_str()).unwrap_or("Done");
            println!("{text}");
            return;
        }

        Cmd::Stats => {
            let db = open_from_project(&project_root).unwrap_or_else(|e| {
                eprintln!("[fog] DB error: {e}"); std::process::exit(1);
            });
            match db.knowledge_score() {
                Ok(s) => {
                    println!("fog-context v{} — Stats for: {}", env!("CARGO_PKG_VERSION"), project_root.display());
                    println!("  Symbols    : {}", s.total_symbols);
                    println!("  Domains    : {}", s.total_domains);
                    println!("  Decisions  : {}", s.total_decisions);
                    println!("  Score      : {}/100", s.layer_score);
                }
                Err(e) => { eprintln!("[fog] Stats error: {e}"); std::process::exit(1); }
            }
            return;
        }

        Cmd::Export { ref format } => {
            let db = open_from_project(&project_root).unwrap_or_else(|e| {
                eprintln!("[fog] DB error: {e}"); std::process::exit(1);
            });
            let fake_args = serde_json::json!({ "format": format, "layer": "all", "max_symbols": 200 });
            let result = tools::inspect::handle(&fake_args, &db);
            let text = result.content.first().map(|c| c.text.as_str()).unwrap_or("{}");
            println!("{text}");
            return;
        }

        Cmd::Serve => {} // fall through to MCP loop
    }

    // ===========================================================
    // MCP Server Mode (default)
    // ===========================================================
    let registry = Registry::load();

    // Open DB
    let db_result = open_from_project(&project_root);
    let db: Arc<Mutex<MemoryDb>> = match db_result {
        Ok(db) => {
            eprintln!("[fog-context] Project: {}", project_root.display());
            Arc::new(Mutex::new(db))
        }
        Err(e) => {
            // Still start the server — fog_roots and fog_scan don't need a DB
            eprintln!("[fog-context] Warning: DB not available ({}). Run fog_scan to index first.", e);
            // Use in-memory DB as fallback so tools that don't need persistence still work
            let fallback = fog_memory::db::MemoryDb::open_empty()
                .expect("in-memory fallback DB must always succeed");
            Arc::new(Mutex::new(fallback))

        }
    };

    eprintln!("[fog-context] Ready. Listening for MCP requests on stdin...");

    // JSON-RPC 2.0 stdio loop
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();

    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line).await {
            Ok(0) => break, // EOF — client disconnected
            Err(e) => {
                eprintln!("[fog-context] stdin error: {e}");
                break;
            }
            Ok(_) => {}
        }

        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        // Parse request
        let response: Value = match serde_json::from_str::<McpRequest>(trimmed) {
            Err(e) => err_response(
                json!(null),
                protocol::ERR_PARSE,
                format!("JSON parse error: {e}"),
            ),
            Ok(req) => {
                let id = req.id.clone().unwrap_or(json!(null));
                match req.method.as_str() {
                    "initialize" => ok_response(id, handle_initialize()),
                    "initialized" => continue, // notification, no response
                    "tools/list" => ok_response(id, handle_list_tools()),
                    "tools/call" => ok_response(
                        id,
                        handle_tools_call(&req.params, &db, &project_root, &registry),
                    ),
                    "ping" => ok_response(id, json!({})),
                    method => err_response(id, ERR_METHOD, format!("Method not found: {method}")),
                }
            }
        };

        // Write response
        let mut out = serde_json::to_string(&response).unwrap_or_default();
        out.push('\n');
        if let Err(e) = stdout.write_all(out.as_bytes()).await {
            eprintln!("[fog-context] stdout error: {e}");
            break;
        }
        let _ = stdout.flush().await;
    }
}
