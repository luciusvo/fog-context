# fog-context - Agentic Codebase Intelligence Engine

> **v0.6.5** | Zero runtime dependency | <5ms cold start | 14 MCP Tools | Rust

fog-context is a **dual-mode binary** that serves as the memory backbone for AI agents working on large codebases. It provides a 5-layer knowledge graph via the Model Context Protocol (MCP), integrating with Cursor, Cline, Claude Desktop, and Zed.

---

## One-time Global Setup (Do this once per machine)

fog-context uses a **single universal binary** at `~/.fog/bin/fog-mcp-server` shared across all your repos. You only install once - every project just points to the same binary.

### Step 1: Download and verify the binary

```bash
# Create the fog home directory
mkdir -p ~/.fog/bin

# Download the binary for your platform
# Linux (x86_64)
curl -L https://github.com/luciusvo/fog-context/releases/latest/download/fog-mcp-linux-amd64 \
  -o ~/.fog/bin/fog-mcp-server && chmod +x ~/.fog/bin/fog-mcp-server

# macOS Apple Silicon (M1/M2/M3)
curl -L https://github.com/luciusvo/fog-context/releases/latest/download/fog-mcp-macos-arm64 \
  -o ~/.fog/bin/fog-mcp-server && chmod +x ~/.fog/bin/fog-mcp-server

# macOS Intel
curl -L https://github.com/luciusvo/fog-context/releases/latest/download/fog-mcp-macos-amd64 \
  -o ~/.fog/bin/fog-mcp-server && chmod +x ~/.fog/bin/fog-mcp-server
```

**Verify the binary works** (mandatory before proceeding):
```bash
ls -la ~/.fog/bin/fog-mcp-server
# Expected: -rwxr-xr-x ... fog-mcp-server
# If permission denied: chmod +x ~/.fog/bin/fog-mcp-server

~/.fog/bin/fog-mcp-server stats --project /tmp 2>&1 | head -3
# Expected: "fog-context v0.6.x - Stats for: /tmp" (or DB error — both confirm binary works)
# If "command not found": the download failed, retry the curl command above
```

After install, your fog home directory looks like:

```
~/.fog/
├── bin/
│   └── fog-mcp-server     ← Universal binary (shared by all repos)
├── logs/
│   └── parser_errors.log  ← Global telemetry for AST query crashes
└── registry.json          ← Auto-created on first fog_brief or CLI index.
```

> [!NOTE]
> `registry.json` is created automatically the FIRST time you run either `fog_brief` (via MCP)
> or `fog-mcp-server index --project /path` (via CLI). It is NOT created at install time.

### Step 2: Configure your AI editor (one-time, works for ALL repos)

fog-context v0.6.5 uses **explicit per-call routing via `fog_id`**. There is no global env var.
Choose the setup that matches your workflow:

---

#### Scenario A — Multi-project mode (recommended for Antigravity, headless agents)

No args at startup. **Each tool call passes `"project": "<fog_id>"`** to route to the right DB.

```json
{
  "mcpServers": {
    "fog-context": {
      "command": "/home/your-username/.fog/bin/fog-mcp-server"
    }
  }
}
```

Agent workflow:
```
# 1. Get fog_id for this project (one-time per project)
fog_brief({ "project": "/absolute/path/to/repo" })
→ response shows: fog_id: `fog_019506...`  ← save this

# 2. Use fog_id in all subsequent calls
fog_brief({ "project": "fog_019506..." })
fog_lookup({ "query": "auth", "project": "fog_019506..." })
```

> [!TIP]
> The fastest way to get fog_id: `cat /path/to/repo/.fog-context/config.toml`
> fog_brief registers the project automatically on first call.

---

#### Scenario B — Single-project mode (Cursor, Zed, dedicated setups)

Use `--project` when you always work on one repo:

**Cursor** (`.cursor/mcp.json`), **Cline** (`cline_mcp_settings.json`), **Claude Desktop**:
```json
{
  "mcpServers": {
    "fog-context": {
      "command": "/home/your-username/.fog/bin/fog-mcp-server",
      "args": ["--project", "/home/your-username/myproject"]
    }
  }
}
```

In single-project mode, omitting `"project"` in tool calls is OK — server uses the configured default.

**Zed** (`~/.config/zed/settings.json`):
```json
{
  "context_servers": {
    "fog-context": {
      "command": {
        "path": "/home/your-username/.fog/bin/fog-mcp-server",
        "args": ["--project", "${ZED_WORKTREE_ROOT}"]
      }
    }
  }
}
```

> [!IMPORTANT]
> `FOG_PROJECT` env var was **removed in v0.6.2** — it caused multi-agent routing contamination.
> Use `"args": ["--project", "/path"]` instead.

> [!TIP]
> Call `fog_brief({})` at session start to verify which project fog-context is serving.
> Output shows **fog_id**, **Name**, **Path**, and whether a **new binary** needs re-indexing.

---

## Updating the Binary

fog-context includes a built-in Version Check mechanism. Every time you or the AI calls `fog_brief`, it compares the running binary's version against the `indexer_version` saved in the project's `.fog-context/config.toml`. If there is a mismatch, `fog_brief` will display a `🆕 VERSION MISMATCH` banner, signaling that an update has occurred and the project may need to be re-indexed to take advantage of new AST features.

To update to the latest version:
1. Download the latest release from the [GitHub Releases](https://github.com/luciusvo/fog-context/releases) page for your OS.
2. Replace your existing binary. By convention, this is located at `~/.fog/bin/fog-mcp-server`.

**Mac / Linux:**
```bash
# Download and replace the binary
mv /path/to/downloaded/fog-mcp-linux-amd64 ~/.fog/bin/fog-mcp-server

# Make it executable again
chmod +x ~/.fog/bin/fog-mcp-server
```

**Windows:**
Replace the old `fog-mcp-windows-amd64.exe` inside your `%USERPROFILE%\.fog\bin\` folder with the newly downloaded `.exe`.

After updating, simply run `fog_scan({ "full": false })` via the AI or `fog-mcp-server index` via CLI to update your project's AST graph to the latest schema!

---


## Adding a New Repo (What AI Agents Should Do)

When you open a new project in your IDE, fog-context is already running. The AI agent needs to initialize the index.

### Agent protocol: starting on a new repo

```
# Step 0: Get fog_id (always first)
fog_brief({ "project": "/absolute/path/to/repo" })
→ Response shows: fog_id + estimated file count
→ IMPORTANT: if "Large project (~N files detected)" warning shown → use CLI (Step 1b)

# Step 1a: For small/medium repos (< 1000 files) — index via MCP:
fog_scan({ "project": "<fog_id from fog_brief>" })

# Step 1b: For large repos (> 1000 files) — index via CLI (shows progress):
# Run in terminal:
fog-mcp-server index --project /absolute/path/to/repo
# Then verify:
fog_brief({ "project": "<fog_id>" })

# Step 2: Verify
fog_roots({})
→ Should show this project in the list

# Step 3: Build knowledge layers (mandatory for full intelligence):
fog_assign({ "domain": "Authentication", "symbols": ["login", "auth_check"] })
fog_constraints({ "path": "." })    ← scans for ADR files
fog_decisions({ "functions": ["key_fn"], "reason": "why it works this way" })
```

### For humans: Quick start on a new repo

```bash
# Using CLI (runs indexer directly, no MCP client needed)
~/.fog/bin/fog-mcp-server --project /path/to/your/repo index

# The repo is now registered and ready:
~/.fog/bin/fog-mcp-server --project /path/to/your/repo stats
```

### Registry: Track all your indexed repos

```bash
# See all registered projects:
cat ~/.fog/registry.json
```

The registry auto-updates after every `fog_scan`. Example output:

```json
[
  {
    "name": "goclaw",
    "path": "/home/admin/Downloads/goclaw",
    "symbol_count": 10824,
    "last_indexed": "2026-04-19T01:30:00Z",
    "db_path": "/home/admin/Downloads/goclaw/.fog-context/context.db",
    "fog_id": "prj_3f8a2c1d"
  },
  {
    "name": "EaseUI-PRD",
    "path": "/home/admin/Downloads/EaseUI-PRD",
    "symbol_count": 305,
    "last_indexed": "2026-04-19T02:15:00Z",
    "db_path": "/home/admin/Downloads/EaseUI-PRD/.fog-context/context.db",
    "fog_id": "prj_a1b2c3d4"
  }
]
```

---

## Per-project File Structure

After `fog_scan` runs on a project, fog-context creates these files:

```
your-project/
├── .fog-id                        ← Stable UUID (survives folder renames)
├── .fog-context/
│   ├── context.db                 ← SQLite knowledge graph (Layers 1-4)
│   ├── AGENTS.md                  ← Auto-generated agent instructions
│   └── hints/<lang>.json (opt)    ← Manual bridges for IoC / Metaprogramming
└── .fogignore (optional)          ← Ignore paths from indexing (like .gitignore)
```

### Optional: `.fog-context/hints/<lang>.json` (Framework Magic)

For frameworks heavily relying on Dependency Injection (IoC), Event Buses, or Metaprogramming (Rails `has_many`), you can supply static hints to bridge edges that the AST cannot naturally see.

```json
// .fog-context/hints/csharp.json
{
  "di_annotations": ["@Inject", "@MyService", "[ApiController]"],
  "extra_calls": [
    { "from": "IUserRepository.Add", "to": "UserRepository.Add" }
  ]
}
```

### Ignoring files: `.fogignore`

`fog-context` automatically ignores standard excluded directories (`.git`, `node_modules`, `target`, `dist`, `build`, etc.) and respects your `.gitignore`.

If you have valid source code files that you **do not** want indexed (e.g. `Research/`, `examples/`, `experiments/`), simply create a `.fogignore` file at the project root:

```text
# .fogignore
Research/
experiments/
drafts/
```

### Custom ADR Paths: `.fog-context/config.toml`

If your project stores ADRs (Architecture Decision Records) somewhere other than standard paths like `logs/decisions/` or `docs/adr/`, you can override this directly in the engine's config file (`.fog-context/config.toml`):

```toml
# .fog-context/config.toml
[adr]
paths = [
  "docs/architecture",
  "knowledge/decisions"
]
```

---

## CLI Reference

```bash
# Index a project (first run or after large changes)
~/.fog/bin/fog-mcp-server --project /path/to/project index

# Force full re-index (ignore checksums)
~/.fog/bin/fog-mcp-server --project /path/to/project index --full

# Print index statistics
~/.fog/bin/fog-mcp-server --project /path/to/project stats

# Export knowledge snapshot (xml | md | json)
~/.fog/bin/fog-mcp-server --project /path/to/project export --format xml

# List all 14 available MCP tools
~/.fog/bin/fog-mcp-server --list-tools
```

---

## Build from Source

```bash
# Minimum Rust version: 1.75+
git clone https://github.com/luciusvo/fog-context.git
cd fog-context
cargo build --release --package fog-mcp-server

# Binary will be at: target/release/fog-mcp-server
# Copy to universal location:
cp target/release/fog-mcp-server ~/.fog/bin/fog-mcp-server

# With mobile language support (Kotlin/Swift/Dart - requires native C toolchain)
cargo build --release --package fog-mcp-server --features all-langs
```

> **macOS Intel users:** No pre-built binary is provided (the `macos-13` GitHub Actions runner
> is currently unavailable). Use the command above to build locally - takes ~2 minutes with
> Rust installed. Alternatively, the ARM64 binary (`fog-mcp-macos-arm64`) runs transparently
> on Intel Macs via Rosetta 2 (macOS 11+).

---

## 14 MCP Tools

| Tool | Purpose | Priority |
|:---|:---|:---|
| `fog_brief` | Index health check - **call first every session** | 🔴 Mandatory |
| `fog_scan` | Index or re-index project with Tree-sitter | Core |
| `fog_lookup` | Full-text search for symbols by name/doc | Core |
| `fog_outline` | Lightweight file outline (names+sigs, no source) | Core |
| `fog_inspect` | 360° symbol context: callers, callees, constraints | Core |
| `fog_impact` | Blast radius analysis before any edit | 🔴 Mandatory |
| `fog_trace` | Full call tree downstream or upstream | Core |
| `fog_roots` | List all indexed projects in ~/.fog/registry.json | Core |
| `fog_gaps` | Find orphans, cycles, dead code | Advanced |
| `fog_domains` | Query business domains and their symbols | Advanced |
| `fog_assign` | Define/update a business domain | Advanced |
| `fog_constraints` | Ingest ADR files AND push inline architecture constraints (Layer 3) | Advanced |
| `fog_decisions` | Record WHY code was changed (builds causality log) | Advanced |
| `fog_import` | Migrate from ByteRover / GitNexus to fog-context | Advanced |

### Tool argument reference (avoid hallucination)

| Tool | Required args | Example |
|:---|:---|:---|
| `fog_scan` | *(none)* | `fog_scan({})` |
| `fog_lookup` | `query` (string) | `fog_lookup({ "query": "auth*" })` |
| `fog_outline` | `path` (string) | `fog_outline({ "path": "src/auth" })` |
| `fog_inspect` | `name` (string) | `fog_inspect({ "name": "verify_token" })` |
| `fog_impact` | `target` (string) | `fog_impact({ "target": "verify_token" })` |
| `fog_trace` | `entry` (string) | `fog_trace({ "entry": "main", "direction": "down" })` |
| `fog_gaps` | `template` (string) | `fog_gaps({ "template": "find_orphans" })` |
| `fog_decisions` | `functions` (array), `reason` (string) | `fog_decisions({ "functions": ["fn_a"], "reason": "..." })` |

---

## Agent Workflow (Mandatory 4-Step Protocol)

Every AI agent session **must** follow this order:

```
1. fog_brief({})           → Verify index is fresh (symbols > 0)
                             If symbols = 0 → call fog_scan first
2. fog_lookup/domains      → Orient to the codebase
3. fog_impact({ target })  → Check blast radius BEFORE any edit
                             If risk = HIGH/CRITICAL → STOP and warn user
4. fog_decisions({...})    → Record WHY after every significant change
```

---

## Language Support

| Language | Extensions | Notes |
|:---|:---|:---|
| Rust | `.rs` | Native |
| TypeScript / JavaScript | `.ts`, `.tsx`, `.js`, `.jsx` | Native (TSX grammar separate) |
| Python | `.py` | Native |
| Go | `.go` | Native |
| C / C++ | `.c`, `.cpp`, `.cc`, `.h` | Native |
| C# | `.cs` | Native |
| Ruby | `.rb` | Native |
| PHP | `.php` | Native |
| Lua | `.lua` | Native |
| **Kotlin** | `.kt`, `.kts` | `--features kotlin` (C-FFI) |
| **Swift** | `.swift` | `--features swift` (C-FFI) |
| **Dart** | `.dart` | `--features dart` (C-FFI) |

---

## 5-Layer Memory Architecture

```
Layer 1: PHYSICAL     ← Auto-indexed (AST symbols, call edges, imports)
Layer 2: BUSINESS     ← fog_domains + fog_assign
Layer 3: CONSTRAINT   ← fog_constraints (from ADR files)
Layer 4: CAUSALITY    ← fog_decisions (WHY code changed)
Layer 5: HARNESS      ← fog_brief (session state + staleness detection)
```

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history and release notes.

---

## License

MIT - See [LICENSE](LICENSE)
