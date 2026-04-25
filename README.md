# fog-context — Agentic Codebase Intelligence Engine

> **v0.6.5** | Zero runtime dependency | <5ms cold start | 14 MCP Tools | Rust

fog-context is a **dual-mode binary** (MCP server + CLI) that serves as the memory backbone for AI agents working on large codebases. It builds a persistent 5-layer knowledge graph and integrates with Cursor, Cline, Claude Desktop, and Zed.

---

## ⚡ Quick Start

1. Install the binary to `~/.fog/bin/fog-mcp-server` (see [Initial Setup](#-initial-setup))
2. Add to your AI Editor's MCP config:
   ```json
   {
     "mcpServers": {
       "fog-context": { "command": "/home/your-user/.fog/bin/fog-mcp-server" }
     }
   }
   ```
3. Open any project and tell your AI: **"Use fog_scan to index this project."**
4. For first-time setups, tell your AI: **"Populate the knowledge layers using fog_assign, fog_constraints, and fog_decisions. Use a mid-tier model if needed."**

---

## 📋 Overview

- [Initial Setup](#-initial-setup) — Install the binary and configure your AI editor (one-time per machine)
- [Per-project Setup](#-per-project-setup) — Initialize a project, control what gets indexed
- [Daily Usage](#-daily-usage) — Updating the binary, adding new repos, CLI reference
- [For AI Agents](#-for-ai-agents) — Mandatory session protocol and tool reference
- [Reference](#-reference) — Language support, 5-layer architecture, tool list

---

## 🔧 Initial Setup

> Do this **once per machine**. Every project then shares the same binary.

### Step 1: Download the binary

fog-context uses a **single universal binary** at `~/.fog/bin/fog-mcp-server` shared across all your repos.

```bash
# Create the fog home directory
mkdir -p ~/.fog/bin

# Linux (x86_64)
curl -L https://github.com/luciusvo/fog-context/releases/latest/download/fog-mcp-linux-amd64 \
  -o ~/.fog/bin/fog-mcp-server && chmod +x ~/.fog/bin/fog-mcp-server

# macOS Apple Silicon (M1/M2/M3/M4)
curl -L https://github.com/luciusvo/fog-context/releases/latest/download/fog-mcp-macos-arm64 \
  -o ~/.fog/bin/fog-mcp-server && chmod +x ~/.fog/bin/fog-mcp-server
```

**Windows:** Download `fog-mcp-windows-amd64.exe` from [GitHub Releases](https://github.com/luciusvo/fog-context/releases) and place it at `%USERPROFILE%\.fog\bin\`.

**Verify the binary works:**
```bash
ls -la ~/.fog/bin/fog-mcp-server
# Expected: -rwxr-xr-x ... fog-mcp-server

~/.fog/bin/fog-mcp-server stats --project /tmp 2>&1 | head -3
# Expected: "fog-context v0.6.x - Stats for: /tmp"
```

After install, your fog home directory looks like:

```
~/.fog/
├── bin/
│   └── fog-mcp-server     ← Universal binary (shared by all repos)
├── logs/
│   └── parser_errors.log  ← Global telemetry for AST query crashes
└── registry.json          ← Auto-created on first fog_brief or CLI index
```

> [!NOTE]
> `registry.json` is created automatically the FIRST time you run either `fog_brief` (via MCP)
> or `fog-mcp-server index --project /path` (via CLI). It is NOT created at install time.

---

### Step 2: Configure your AI editor

fog-context v0.6.5 uses **explicit per-call routing via `fog_id`**. There is no global env var.

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
fog_lookup({ "query": "auth", "project": "fog_019506..." })
```

> [!TIP]
> The fastest way to get fog_id: `cat /path/to/repo/.fog-context/config.toml`

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

---

## 📁 Per-project Setup

After `fog_scan` runs on a project, fog-context creates these files inside it:

```
your-project/
├── .fog-context/
│   ├── context.db                 ← SQLite knowledge graph (Layers 1-4)
│   ├── AGENTS.md                  ← Auto-generated agent instructions
│   └── hints/<lang>.json (opt)    ← Manual bridges for IoC / Metaprogramming
└── .fogignore (optional)          ← Exclude directories from indexing
```

---

### Controlling What Gets Indexed

#### How the file ignore system works

fog-context uses a layered ignore system — files are filtered in this priority order:

| Layer | Mechanism | What it blocks |
|:------|:----------|:---------------|
| 1 | **Built-in hardcoded list** | `.git`, `node_modules`, `target`, `dist`, `build`, `.venv`, `__pycache__`, `.next`, `vendor`, `tmp`, `coverage`, `.fog-context` |
| 2 | **`.gitignore` / `.ignore`** | Any path listed in your standard gitignore (read automatically) |
| 3 | **Extension filter** | Any file whose extension is not a supported language (`.md`, `.json`, `.png`, images, etc.) |
| 4 | **`.fogignore`** | Your custom exclusions — for valid source code dirs you don't want indexed |

> [!IMPORTANT]
> Layers 1–3 run automatically with zero configuration. **However**, if your project has valid source code
> directories that should NOT be part of the indexed codebase (e.g. a `Research/` folder with cloned
> third-party repos, or an `experiments/` directory), you **must** create a `.fogignore` file to exclude them.
> Without it, fog-context will parse every `.rs`, `.py`, `.ts`… file it finds — even in those directories.

#### Example: when to use `.fogignore`

A project reporting **10,000+ files** but with only ~1,000 real application files is a strong signal.
This typically happens when the repo contains directories with valid code extensions that aren't part
of the application (research, snapshots, vendored examples).

```
# .fogignore  (placed at the project root, same level as .gitignore)
Research/
experiments/
docs/vendor/
```

`.fogignore` uses the exact same syntax as `.gitignore`. After adding or modifying it, run `fog_scan` with `full=true` to re-index from scratch.

#### Custom ADR Paths

If your project stores ADRs (Architecture Decision Records) somewhere other than the standard paths
(`logs/decisions/`, `docs/adr/`, `docs/decisions/`), you can override this in `.fog-context/config.toml`:

```toml
# .fog-context/config.toml
[adr]
paths = [
  "docs/architecture",
  "knowledge/decisions"
]
```

#### Framework Magic: `.fog-context/hints/<lang>.json`

For frameworks using Dependency Injection (IoC), Event Buses, or Metaprogramming (Rails `has_many`),
AST analysis cannot see the runtime links. Bridge them manually:

```json
// .fog-context/hints/csharp.json
{
  "di_annotations": ["@Inject", "@MyService", "[ApiController]"],
  "extra_calls": [
    { "from": "IUserRepository.Add", "to": "UserRepository.Add" }
  ]
}
```

After creating or editing a hints file, run `fog_scan({ "full": false })` to apply.

---

## 🔄 Daily Usage

### Updating the Binary

fog-context has a **built-in version check**: every `fog_brief` call compares the running binary's version
against `indexer_version` in `.fog-context/config.toml`. A `🆕 VERSION MISMATCH` banner appears if the
binary is newer than the last index.

**To update:**

1. Download the new release from [GitHub Releases](https://github.com/luciusvo/fog-context/releases)
2. Replace the existing binary:

**Mac / Linux:**
```bash
mv /path/to/downloaded/fog-mcp-linux-amd64 ~/.fog/bin/fog-mcp-server
chmod +x ~/.fog/bin/fog-mcp-server
```

**Windows:** Replace `fog-mcp-windows-amd64.exe` inside `%USERPROFILE%\.fog\bin\`.

3. Re-index your projects to upgrade the AST graph:
```bash
fog_scan({ "full": false })   # via AI
# or
~/.fog/bin/fog-mcp-server index --project /path/to/project   # via CLI
```

---

### Adding a New Repo

When you open a new project, the AI agent needs to initialize the index.

**Agent protocol:**
```
# Step 0: Get fog_id (always first)
fog_brief({ "project": "/absolute/path/to/repo" })
→ Response shows: fog_id + estimated file count
→ IMPORTANT: if "Large project (~N files detected)" warning shown → use CLI (Step 1b)

# Step 1a: For small/medium repos (< 1000 files) — index via MCP:
fog_scan({ "project": "<fog_id from fog_brief>" })

# Step 1b: For large repos (> 1000 files) — index via CLI (shows progress):
fog-mcp-server index --project /absolute/path/to/repo
# Then verify:
fog_brief({ "project": "<fog_id>" })

# Step 2: Build knowledge layers (mandatory for full intelligence):
fog_assign({ "domain": "Authentication", "symbols": ["login", "auth_check"] })
fog_constraints({ "path": "." })    ← scans for ADR files
fog_decisions({ "functions": ["key_fn"], "reason": "why it works this way" })
```

**For humans (CLI):**
```bash
~/.fog/bin/fog-mcp-server --project /path/to/your/repo index
~/.fog/bin/fog-mcp-server --project /path/to/your/repo stats
```

---

### CLI Reference

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

## 🤖 For AI Agents

### Mandatory Session Protocol

Every AI agent session **must** follow this order:

```
1. fog_brief({})           → Verify index is fresh (symbols > 0)
                             If symbols = 0 → call fog_scan first
2. fog_lookup/domains      → Orient to the codebase
3. fog_impact({ target })  → Check blast radius BEFORE any edit
                             If risk = HIGH/CRITICAL → STOP and warn user
4. fog_decisions({...})    → Record WHY after every significant change
```

### 14 MCP Tools

| Tool | Purpose | Priority |
|:---|:---|:---|
| `fog_brief` | Index health check — **call first every session** | 🔴 Mandatory |
| `fog_scan` | Index or re-index project with Tree-sitter | Core |
| `fog_lookup` | Full-text search for symbols by name/doc | Core |
| `fog_outline` | Lightweight file outline (names+sigs, no source) | Core |
| `fog_inspect` | 360° symbol context: callers, callees, constraints | Core |
| `fog_impact` | Blast radius analysis before any edit | 🔴 Mandatory |
| `fog_trace` | Full call tree downstream or upstream | Core |
| `fog_roots` | List all indexed projects in `~/.fog/registry.json` | Core |
| `fog_gaps` | Find orphans, cycles, dead code | Advanced |
| `fog_domains` | Query business domains and their symbols | Advanced |
| `fog_assign` | Define/update a business domain | Advanced |
| `fog_constraints` | Ingest ADR files + push inline architecture constraints | Advanced |
| `fog_decisions` | Record WHY code was changed (builds causality log) | Advanced |
| `fog_import` | Migrate from ByteRover / GitNexus to fog-context | Advanced |

### Tool Argument Reference

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

## 📚 Reference

### Language Support

| Language | Extensions | Notes |
|:---|:---|:---|
| Rust | `.rs` | Native |
| TypeScript / JavaScript | `.ts`, `.tsx`, `.js`, `.jsx` | Native |
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

### 5-Layer Memory Architecture

```
Layer 1: PHYSICAL     ← Auto-indexed (AST symbols, call edges, imports)
Layer 2: BUSINESS     ← fog_domains + fog_assign
Layer 3: CONSTRAINT   ← fog_constraints (from ADR files)
Layer 4: CAUSALITY    ← fog_decisions (WHY code changed)
Layer 5: HARNESS      ← fog_brief (session state + staleness detection)
```

### Build from Source

```bash
# Minimum Rust version: 1.75+
git clone https://github.com/luciusvo/fog-context.git
cd fog-context
cargo build --release --package fog-mcp-server

# Binary will be at: target/release/fog-mcp-server
cp target/release/fog-mcp-server ~/.fog/bin/fog-mcp-server

# With mobile language support (Kotlin/Swift/Dart - requires native C toolchain)
cargo build --release --package fog-mcp-server --features all-langs
```

> **macOS Intel users:** No pre-built binary is provided. Build locally (~2 min with Rust installed).
> Alternatively, the ARM64 binary (`fog-mcp-macos-arm64`) runs transparently on Intel via Rosetta 2 (macOS 11+).

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history and release notes.

---

## License

MIT — See [LICENSE](LICENSE)
