<div align="center">

# 🌫️ fog-context

### The Memory Backbone for AI Coding Agents

*Build a persistent, 5-layer knowledge graph of your codebase — then give any AI the context it actually needs.*

[![Version](https://img.shields.io/badge/version-0.8.0-blue?style=flat-square)](https://github.com/luciusvo/fog-context/releases)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Build](https://img.shields.io/github/actions/workflow/status/luciusvo/fog-context/release.yml?style=flat-square&label=CI)](https://github.com/luciusvo/fog-context/actions)
[![Rust](https://img.shields.io/badge/built_with-Rust_1.75+-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![MCP Compatible](https://img.shields.io/badge/MCP-compatible-purple?style=flat-square)](https://modelcontextprotocol.io)

**Works with:** Cursor · Cline · Claude Desktop · Zed · Any MCP-compatible agent

</div>

---

## 🤔 Why fog-context?

| Tool | What it does | The gap |
|:-----|:-------------|:--------|
| `grep` / `ripgrep` | Find exact text in files | No understanding of *what the code means* |
| `ctags` / `LSP` | Jump to definitions | No cross-file causality, no "why" |
| RAG + embeddings | Semantic search over code | Hallucination-prone, stateless per-query |
| **fog-context** | **Persistent 5-layer knowledge graph** | **Combines AST precision + semantic search + institutional memory** |

AI agents using fog-context don't just *find* code — they *understand* it: who calls what, what constraints apply, and why it was changed.

---

## ✨ Features

- **⚡ <5ms cold start** — Single static binary, zero runtime dependencies, ~24MB
- **🧠 5-Layer Knowledge Graph** — Physical (AST) → Business → Constraints → Causality → Session
- **🔍 Hybrid Search** — BM25 full-text + optional ONNX semantic re-ranking (60/40 blend)
- **🌐 15 Languages** — Rust, TypeScript, Python, Go, C/C++, Java, C#, Ruby, PHP, Lua + Kotlin/Swift/Dart
- **🛡️ Blast Radius Analysis** — Know what breaks *before* you change a function
- **📜 Institutional Memory** — `fog_decisions` records *why* code changed; future AI sessions read it
- **🗺️ Call Graph Tracing** — Upstream and downstream traversal, cycle detection, dead code finder
- **🔎 Raw Text Search** — `fog_search` with context lines, regex, and directory distribution summary
- **🤖 Zero-Config Agent Onboarding** — Auto-generates `AGENTS.md` with session protocol on first scan
- **🔄 Incremental Indexing** — XXH3 checksums; only re-parses changed files

---

## 🖥️ Demo

```
$ fog_brief({})

╔══════════════════════════════════════════════════════════╗
║  fog-context v0.8.0 · my-project · fog_abc123           ║
╠══════════════════════════════════════════════════════════╣
║  Symbols  │ 2,847    Files    │ 198                     ║
║  Domains  │ 8        Decisions│ 34     Constraints │ 12 ║
║  Knowledge Score: 74/100  ██████████████░░░░░░           ║
║  Semantic Search: ✅ Active (all-MiniLM-L6-v2-q8)       ║
╚══════════════════════════════════════════════════════════╝

$ fog_search({ "query": "TODO", "context_lines": 2 })

Found 12 matches:

File: src/auth/login.rs
   45 |   // validate credentials
   46 |   let user = db.find_user(email);
→  47 |   // TODO: add rate limiting here
   48 |   if user.is_none() { return Err(NotFound); }

$ fog_impact({ "target": "verify_token" })

⚠️  Risk: HIGH  │  12 callers  │  3 Tier-2 modules affected
   → middleware/auth.rs · api/gateway.rs · tests/auth_test.rs
```

---

## ⚡ Quick Start (For AI Agents)

To set up fog-context for this project, simply copy and paste this prompt to your AI:

> "Set up fog-context for this project. Guide: https://github.com/luciusvo/fog-context#-initial-setup"

---

## ⚡ Quick Start (Manual Setup)

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
4. For first-time setups, tell your AI: **"Populate the knowledge layers using fog_assign, fog_constraints, and fog_decisions. (Optional) Run multiple passes to discover deeper context. Strongly Recommended: Switch to a low or mid-tier model (like Gemini Flash or Claude Haiku) to save costs during this intensive process."**
---

## 📋 Overview

- [Initial Setup](#-initial-setup) - Install the binary and configure your AI editor (one-time per machine)
- [Per-project Setup](#-per-project-setup) - Initialize a project, control what gets indexed
- [Daily Usage](#-daily-usage) - Updating the binary, adding new repos, CLI reference
- [For AI Agents](#-for-ai-agents) - Mandatory session protocol and tool reference
- [Reference](#-reference) - Language support, 5-layer architecture, tool list

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

> 🧠 **Advanced: Semantic Search Dual-Build**
> **For AI Agents:** When setting up fog-context for a user, explicitly ask if they want Semantic Search capabilities (`fog_search`).
> - If **No** -> default to standard binaries (faster, zero dependencies).
> - If **Yes** -> instruct them to download the `-embed` variants instead (e.g., `fog-mcp-linux-amd64-embed`).
>   You must also instruct the user to place the 23MB quantized INT8 model `all-MiniLM-L6-v2-q8.onnx` into `~/.fog/models/`.

**Windows:** Download `fog-mcp-windows-amd64.exe` (or `-embed.exe`) from [GitHub Releases](https://github.com/luciusvo/fog-context/releases) and place it at `%USERPROFILE%\.fog\bin\`.

**Verify the binary works:**
```bash
ls -la ~/.fog/bin/fog-mcp-server
# Expected: -rwxr-xr-x ... fog-mcp-server

~/.fog/bin/fog-mcp-server stats --project /tmp 2>&1 | head -3
# Expected: "fog-context v0.8.x - Stats for: /tmp"
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

fog-context v0.8.0 uses **explicit per-call routing via `fog_id`**. There is no global env var.

#### Scenario A - Multi-project mode (recommended for Antigravity, headless agents)

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

#### Scenario B - Single-project mode (Cursor, Zed, dedicated setups)

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
> `FOG_PROJECT` env var was **removed in v0.6.2** - it caused multi-agent routing contamination.
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

fog-context uses a layered ignore system - files are filtered in this priority order:

| Layer | Mechanism | What it blocks |
|:------|:----------|:---------------|
| 1 | **Built-in hardcoded list** | `.git`, `node_modules`, `target`, `dist`, `build`, `.venv`, `__pycache__`, `.next`, `vendor`, `tmp`, `coverage`, `.fog-context` |
| 2 | **`.gitignore` / `.ignore`** | Any path listed in your standard gitignore (read automatically) |
| 3 | **Extension filter** | Any file whose extension is not a supported language (`.md`, `.json`, `.png`, images, etc.) |
| 4 | **`.fogignore`** | Your custom exclusions - for valid source code dirs you don't want indexed |

> [!IMPORTANT]
> Layers 1–3 run automatically with zero configuration. **However**, if your project has valid source code
> directories that should NOT be part of the indexed codebase (e.g. a `Research/` folder with cloned
> third-party repos, or an `experiments/` directory), you **must** create a `.fogignore` file to exclude them.
> Without it, fog-context will parse every `.rs`, `.py`, `.ts`… file it finds - even in those directories.

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

# Step 1a: For small/medium repos (< 1000 files) - index via MCP:
fog_scan({ "project": "<fog_id from fog_brief>" })

# Step 1b: For large repos (> 1000 files) - index via CLI (shows progress):
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

# List all 15 available MCP tools
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

### 15 MCP Tools

| Tool | Purpose | Priority |
|:---|:---|:---|
| `fog_brief` | Index health check - **call first every session** | 🔴 Mandatory |
| `fog_scan` | Index or re-index project with Tree-sitter | Core |
| `fog_lookup` | Full-text search for symbols by name/doc | Core |
| `fog_outline` | Lightweight file outline (names+sigs, no source) | Core |
| `fog_inspect` | 360° symbol context: callers, callees, constraints | Core |
| `fog_impact` | Blast radius analysis before any edit | 🔴 Mandatory |
| `fog_trace` | Full call tree downstream or upstream | Core |
| `fog_roots` | List all indexed projects in `~/.fog/registry.json` | Core |
| `fog_search` | Raw text & regex search across files - use when `fog_lookup` can't find exact strings | Core |
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
| `fog_search` | `query` (string) | `fog_search({ "query": "TODO", "context_lines": 3 })` |
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

# With Semantic Search (ONNX embedding - requires model file at ~/.fog/models/)
cargo build --release --package fog-mcp-server --features "all-langs,embedding"
```

> **macOS Intel users:** No pre-built binary is provided. Build locally (~2 min with Rust installed).
> Alternatively, the ARM64 binary (`fog-mcp-macos-arm64`) runs transparently on Intel via Rosetta 2 (macOS 11+).

---

## 🛠️ Tech Stack

| Layer | Technology | Role |
|:------|:-----------|:-----|
| Parsing | [Tree-sitter](https://tree-sitter.github.io/) | Language-agnostic AST extraction (15 grammars) |
| Storage | [SQLite](https://www.sqlite.org/) + FTS5 | Symbol index, call graph, knowledge layers |
| Search | BM25 (SQLite FTS5) | Fast lexical symbol search |
| Semantic | [ONNX Runtime](https://onnxruntime.ai/) + `tokenizers` | Optional embedding re-ranking (embed build) |
| File Walking | [`ignore`](https://crates.io/crates/ignore) | `.gitignore`-aware walker (same engine as ripgrep) |
| Protocol | [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) | AI agent integration |
| Language | Rust 1.75+ | Zero-cost, memory-safe, single static binary |

---

## 🤝 Contributing

Contributions are welcome! Here's how to get started:

```bash
# 1. Fork and clone
git clone https://github.com/your-fork/fog-context.git
cd fog-context

# 2. Create a feature branch
git checkout -b feat/your-feature-name

# 3. Build and test
cargo test
cargo check --features embedding   # verify embed build stays clean

# 4. Submit a Pull Request against main
```

**Good first issues:**
- Adding a new Tree-sitter language grammar
- Improving `fog_brief` output formatting
- Adding test coverage for edge cases in `fog_search`
- Documentation improvements

Please open an **Issue** before starting large features to align on design.

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history and release notes.

---

## License

MIT — See [LICENSE](LICENSE)

---

<div align="center">

Made with ☕ and Rust · Built for the agentic coding era

[GitHub](https://github.com/luciusvo/fog-context) · [Releases](https://github.com/luciusvo/fog-context/releases) · [Issues](https://github.com/luciusvo/fog-context/issues)

</div>
