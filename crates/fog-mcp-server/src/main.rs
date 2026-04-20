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
mod stale;
mod tools;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fog_memory::{MemoryDb, open_from_project, create_or_open_db};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use protocol::{McpRequest, ToolCallResult, err_response, ok_response, ERR_METHOD};
use registry::Registry;

// ---------------------------------------------------------------------------
// #1: DbPool — per-project DB connection cache
// PATTERN_DECISION: Level 4 (Simple Class with mutable state)
// Justification: needs to track open DB connections across tool calls.
// DI-safe: constructed in main(), passed by &mut ref (no global state).
// ---------------------------------------------------------------------------

/// Caches open MemoryDb instances keyed by project root PathBuf.
/// Allows any tool to route to the correct project via `args["project"]`.
///
/// Memory management:
/// - LRU eviction: least-recently-used non-default connection is evicted when pool is full.
/// - Idle TTL: connections unused for > IDLE_TTL_SECS are closed on next gc_idle() call.
/// - max_open: capped at 4 (was 8) — each SQLite connection uses ~2MB page cache.
struct DbPool {
    /// Map of project_root → (db, last_used_instant)
    pools: std::collections::HashMap<PathBuf, (Arc<Mutex<MemoryDb>>, std::time::Instant)>,
    /// LRU order: front = least recently used, back = most recently used
    lru: std::collections::VecDeque<PathBuf>,
    /// Default project root. None = multi-project mode, no implicit default.
    default_root: Option<PathBuf>,
    /// Maximum number of simultaneously open connections.
    max_open: usize,
}

/// Close connections idle longer than this duration.
const IDLE_TTL_SECS: u64 = 600; // 10 minutes

impl DbPool {
    fn new(default_root: Option<PathBuf>, default_db: Option<Arc<Mutex<MemoryDb>>>) -> Self {
        let mut pools = std::collections::HashMap::new();
        let mut lru = std::collections::VecDeque::new();
        if let (Some(ref root), Some(db)) = (&default_root, default_db) {
            pools.insert(root.clone(), (db, std::time::Instant::now()));
            lru.push_back(root.clone());
        }
        Self { pools, lru, default_root, max_open: 4 }
    }

    /// Record access for LRU tracking.
    fn touch(&mut self, root: &Path) {
        self.lru.retain(|p| p != root);
        self.lru.push_back(root.to_path_buf());
        if let Some((_, ref mut ts)) = self.pools.get_mut(root) {
            *ts = std::time::Instant::now();
        }
    }

    /// Close connections that have been idle longer than IDLE_TTL_SECS.
    /// Keeps the default connection alive regardless of idle time.
    fn gc_idle(&mut self) {
        let ttl = std::time::Duration::from_secs(IDLE_TTL_SECS);
        let default = self.default_root.clone();
        let to_remove: Vec<PathBuf> = self.pools.iter()
            .filter(|(path, (_, ts))| {
                let is_default = default.as_deref() == Some(path.as_path());
                !is_default && ts.elapsed() > ttl
            })
            .map(|(k, _)| k.clone())
            .collect();
        for k in to_remove {
            self.pools.remove(&k);
            self.lru.retain(|p| p != &k);
            eprintln!("[fog-pool] Closed idle connection: {}", k.display());
        }
    }

    /// Evict the least-recently-used non-default connection.
    fn evict_lru(&mut self) {
        let default = self.default_root.as_deref();
        let candidate = self.lru.iter()
            .find(|p| default != Some(p.as_path()))
            .cloned();
        if let Some(k) = candidate {
            self.pools.remove(&k);
            self.lru.retain(|p| p != &k);
            eprintln!("[fog-pool] LRU evicted: {}", k.display());
        }
    }

    /// Force-close a specific connection (e.g., after fog_scan to release write lock).
    /// Next get_or_open() will reopen clean.
    pub fn evict(&mut self, root: &Path) {
        self.pools.remove(root);
        self.lru.retain(|p| p != root);
    }

    /// Get cached DB for `root`, or open a new one.
    fn get_or_open(&mut self, root: &Path) -> Arc<Mutex<MemoryDb>> {
        // GC idle connections on every access (no background thread needed)
        self.gc_idle();

        if let Some((db, _)) = self.pools.get(root) {
            let db = db.clone();
            self.touch(root);
            return db;
        }
        // Pool full → LRU eviction
        if self.pools.len() >= self.max_open {
            self.evict_lru();
        }
        let db = create_or_open_db(root)
            .or_else(|_| open_from_project(root))
            .unwrap_or_else(|_| fog_memory::db::MemoryDb::open_empty()
                .expect("in-memory fallback must always succeed"));
        let arc = Arc::new(Mutex::new(db));
        self.pools.insert(root.to_path_buf(), (arc.clone(), std::time::Instant::now()));
        self.lru.push_back(root.to_path_buf());
        arc
    }

    fn get_default(&self) -> Option<Arc<Mutex<MemoryDb>>> {
        self.default_root.as_ref().and_then(|r| self.pools.get(r).map(|(db, _)| db.clone()))
    }

    fn default_root(&self) -> Option<&Path> {
        self.default_root.as_deref()
    }
}

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
    pool: &mut DbPool,
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

    // #1: Per-request project routing via fog_id / name / path-suffix / fuzzy
    let (effective_db, effective_root) = if let Some(key) = args["project"].as_str() {
        match registry.find(key) {
            Some(entry) => {
                let root = PathBuf::from(&entry.path);
                let db = pool.get_or_open(&root);
                (db, root)
            }
            None => {
                // G1: fog_scan and fog_brief may receive an absolute path to a
                // project not yet in the registry (first-time index). Allow it.
                let is_scan_tool = tool_name == "fog_scan" || tool_name == "fog_brief";
                let as_path = PathBuf::from(key);
                if is_scan_tool && as_path.is_absolute() && as_path.exists() {
                    let db = pool.get_or_open(&as_path);
                    (db, as_path)
                } else {
                    let known = registry.list_names();
                    let known_str = if known.is_empty() {
                        "(none — run fog_scan in a project directory first)".to_string()
                    } else {
                        known.iter().map(|n| format!("  - `{n}`")).collect::<Vec<_>>().join("\n")
                    };
                    let result = crate::protocol::ToolCallResult::err(format!(
                        "ESCALATE_MISSING_CONTEXT\n\
                         Project `{key}` not found in fog registry (tried 6-tier fuzzy match).\n\
                         Known projects:\n{known_str}\n\n\
                         ℹ️  Fix options:\n\
                         1. Run fog_scan in the project root to register it first.\n\
                         2. Use the exact registered name/fog_id shown above.\n\
                         3. Omit `project` arg to use default: {}",
                        pool.default_root().map(|p| p.display().to_string())
                            .unwrap_or_else(|| "(none)".to_string())
                    ));
                    return json!({
                        "content": result.content,
                        "isError": result.is_error,
                    });
                }
            }
        }
    } else {
        // No explicit `project` arg — use default if available
        match (pool.get_default(), pool.default_root()) {
            (Some(db), Some(root)) => (db, root.to_path_buf()),
            _ => {
                // No default configured (multi-project mode without --project)
                // Fail-loud: agent must pass explicit project arg
                let known = registry.list_names();
                let known_str = if known.is_empty() {
                    "(none — run fog_scan with project path first)".to_string()
                } else {
                    known.iter().map(|n| format!("  - `{n}`")).collect::<Vec<_>>().join("\n")
                };
                let result = crate::protocol::ToolCallResult::err(format!(
                    "ESCALATE_NO_DEFAULT_PROJECT\n\
                     No default project configured. fog-context is running in multi-project mode.\n\n\
                     You must pass a fog_id or project name with every call:\n\
                     Example: {{ \"project\": \"fog_019506a8b3f8...\" }}\n\n\
                     Known projects:\n{known_str}\n\n\
                     ℹ️  First time? Bootstrap a project:\n\
                     1. fog_scan({{ \"project\": \"/absolute/path/to/project\" }})\n\
                     2. Capture the fog_id from the response.\n\
                     3. Pass it in all subsequent calls."
                ));
                return json!({
                    "content": result.content,
                    "isError": result.is_error,
                });
            }
        }
    };

    let result = router::dispatch(tool_name, args, &effective_db, &effective_root, registry);

    // A4: After fog_scan, evict the write-heavy connection so the next query
    // gets a fresh reader. Prevents query-tools from blocking on scan's WAL.
    if tool_name == "fog_scan" && !result.is_error {
        pool.evict(&effective_root);
    }

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
///   [REMOVED P4: FOG_PROJECT env var — caused multi-agent routing contamination]
/// Returns None in MCP mode when no project is resolvable (multi-project setup).
/// Returns CWD fallback in CLI mode only.
fn resolve_project_root_opt(explicit: Option<PathBuf>, is_cli: bool) -> Option<PathBuf> {
    // P1: Explicit --project arg
    if let Some(p) = explicit {
        return Some(p);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // P2/P3: Probe CWD and ancestors for .fog-context/
    if let Some(root) = find_project_root_from(&cwd) {
        return Some(root);
    }

    // CLI mode only: CWD fallback (human running a command from a directory)
    if is_cli {
        eprintln!("[fog-context] INFO: No .fog-context/ found. Using CWD: {}", cwd.display());
        eprintln!("[fog-context]      Run fog_scan to index this directory.");
        return Some(cwd);
    }

    // MCP mode: no fallback — callers must pass explicit project arg per-request
    eprintln!("[fog-context] Starting in multi-project mode (no default project).");
    eprintln!("[fog-context] Pass fog_id with every call: {{ \"project\": \"fog_...\" }}");
    None
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

    // Determine if this is CLI (non-interactive) mode or MCP server mode
    let is_cli = !matches!(args.cmd, Cmd::Serve);

    // Resolve project root — None = multi-project MCP mode (no default)
    let project_root_opt = resolve_project_root_opt(args.project, is_cli);

    // ===========================================================
    // CLI sub-command dispatch (non-interactive, exits when done)
    // ===========================================================
    // CLI sub-commands need a resolved project root — exit if none found
    let cli_root = || -> PathBuf {
        project_root_opt.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        })
    };

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
            let project_root = cli_root();
            eprintln!("[fog] Indexing: {} (full={})", project_root.display(), full);
            let db = create_or_open_db(&project_root).unwrap_or_else(|e| {
                eprintln!("[fog] DB error: {e}"); std::process::exit(1);
            });
            // Fix 2: eager fog_id — generate it now, before index runs
            let fog_id = crate::registry::ensure_project_id(&project_root.to_string_lossy());
            eprintln!("[fog] fog_id: {fog_id}");
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
            let project_root = cli_root();
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
            let project_root = cli_root();
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

    // A1: Close inherited file descriptors from Electron parent process.
    // Electron spawns fog-mcp-server and passes all its FDs (LevelDB, GPU cache, etc.).
    // We only need stdin(0), stdout(1), stderr(2).
    // Linux 5.9+: close_range(3, u32::MAX, 0) closes all FDs >= 3 atomically.
    #[cfg(target_os = "linux")]
    unsafe {
        // CLOSE_RANGE_CLOEXEC not needed since we're closing, not marking
        // SYS_close_range = 436 on x86_64
        let ret = libc::syscall(436, 3u32, u32::MAX, 0u32);
        if ret < 0 {
            // Fallback: iterate /proc/self/fd and close individually
            if let Ok(dir) = std::fs::read_dir("/proc/self/fd") {
                for entry in dir.flatten() {
                    if let Ok(fd_str) = entry.file_name().into_string() {
                        if let Ok(fd) = fd_str.parse::<i32>() {
                            if fd > 2 {
                                let _ = libc::close(fd);
                            }
                        }
                    }
                }
            }
        }
    }


    // Open default DB (only if a project root was resolved)
    // Fix 2: also ensure fog_id exists immediately — before index runs
    let registry = Registry::load();

    let (default_db, default_root) = match project_root_opt {
        Some(ref root) => {
            // Eager fog_id: generate now so agents can capture it from fog_brief
            let fog_id = crate::registry::ensure_project_id(&root.to_string_lossy());
            let db = match create_or_open_db(root) {
                Ok(db) => {
                    eprintln!("[fog-context] Default project: {} (fog_id: {})", root.display(), fog_id);
                    Arc::new(Mutex::new(db))
                }
                Err(e) => {
                    eprintln!("[fog-context] Warning: DB not available ({e}). Run fog_scan to index.");
                    Arc::new(Mutex::new(
                        fog_memory::db::MemoryDb::open_empty()
                            .expect("in-memory fallback must always succeed")
                    ))
                }
            };
            (Some(db), project_root_opt)
        }
        None => {
            // Multi-project mode: no default
            (None, None)
        }
    };

    // #1: Wrap in DbPool for multi-project routing
    let mut pool = DbPool::new(default_root, default_db);

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
                        handle_tools_call(&req.params, &mut pool, &registry),
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
