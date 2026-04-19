//! fog-mcp-server - Dual-mode binary: MCP server + CLI tool.
//!
//! fog-context v0.5.0 - Rust rewrite of the TypeScript fog-context-repo.
//!
//! ## MCP Mode (Default - for AI Agents)
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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fog_memory::{MemoryDb, open_from_project, create_or_open_db};
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
                // B4 fix: scan ALL remaining args for --full, not just positional i+1
                let full = raw[i..].iter().any(|s| s == "--full" || s == "-f");
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

// ---------------------------------------------------------------------------
// F2: Smart project root resolution
// ---------------------------------------------------------------------------

/// Resolve the project root using a priority chain:
///   P1: Explicit --project arg
///   P2: CWD has .fog-context/ → valid project root
///   P3: Walk up ancestors to find .fog-context/
///   P4: FOG_PROJECT env var (for headless agents / CI)
///   P5: CWD fallback with WARNING log
fn resolve_project_root(explicit: Option<PathBuf>) -> PathBuf {
    // P1: Explicit --project arg
    if let Some(p) = explicit {
        return p;
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // P2/P3: Probe CWD and ancestors for .fog-context/
    if let Some(root) = find_project_root_from(&cwd) {
        return root;
    }

    // P4: FOG_PROJECT env var
    if let Ok(p) = std::env::var("FOG_PROJECT") {
        let path = PathBuf::from(&p);
        if path.exists() {
            eprintln!("[fog-context] Using FOG_PROJECT env: {}", path.display());
            return path;
        } else {
            eprintln!("[fog-context] WARNING: FOG_PROJECT='{}' does not exist, ignoring.", p);
        }
    }

    // P5: CWD fallback — warn clearly
    eprintln!("[fog-context] INFO: No .fog-context/ found from CWD '{}'.", cwd.display());
    eprintln!("[fog-context]      Will create new project index here on fog_scan.");
    eprintln!("[fog-context]      Or set: FOG_PROJECT=/path/to/repo  to point to existing project.");
    cwd
}

/// Walk up directory tree from `start` looking for a `.fog-context/` subdirectory.
/// Returns the directory that contains .fog-context/, or None if not found.
fn find_project_root_from(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".fog-context").is_dir() {
            return Some(current);
        }
        // Legacy: .fog-id at project root
        if current.join(".fog-id").exists() {
            return Some(current);
        }
        match current.parent() {
            Some(p) => {
                let p: PathBuf = p.to_path_buf();
                if p == current { return None; }
                current = p;
            }
            None => return None,
        }
    }
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    // Resolve project root
    let project_root = resolve_project_root(args.project);

    // ===========================================================
    // CLI sub-command dispatch (non-interactive, exits when done)
    // ===========================================================
    match args.cmd {
        Cmd::ListTools => {
            let tools = router::list_tools();
            println!("fog-context v{} - {} tools available:", env!("CARGO_PKG_VERSION"), tools.len());
            for t in &tools {
                println!("  {} - {}", t.name, t.description.lines().next().unwrap_or(""));
            }
            return;
        }

        Cmd::Index { full } => {
            eprintln!("[fog] Indexing: {} (full={})", project_root.display(), full);
            // create_or_open_db: bootstraps .fog-context/context.db if first run
            let db = create_or_open_db(&project_root).unwrap_or_else(|e| {
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
                    println!("fog-context v{} - Stats for: {}", env!("CARGO_PKG_VERSION"), project_root.display());
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
            let fmt = format.clone();
            let fake_args = serde_json::json!({ "format": fmt });
            let reg = Registry::load();
            let result = tools::brief::handle(&fake_args, &db, &reg, &project_root);
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

    // Open DB - use create_or_open_db so fog_scan can work on a fresh project
    // without requiring the user to run CLI `index` first.
    let db_result = create_or_open_db(&project_root);
    let db: Arc<Mutex<MemoryDb>> = match db_result {
        Ok(db) => {
            eprintln!("[fog-context] Project: {}", project_root.display());
            Arc::new(Mutex::new(db))
        }
        Err(e) => {
            // Genuine error (e.g. permissions) - still start the server with fallback
            eprintln!("[fog-context] Warning: DB not available ({}). Run fog_scan to index first.", e);
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
            Ok(0) => break, // EOF - client disconnected
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
                    // B5 fix: notifications are fire-and-forget, must never get a response
                    m if m.starts_with("notifications/") || m == "initialized" => continue,
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
