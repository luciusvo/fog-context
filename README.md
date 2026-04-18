# fog-context — Agentic Codebase Intelligence Engine

> **v0.5.3 — Rust Rewrite** | Zero runtime dependency | <5ms cold start | 14 MCP Tools

fog-context is a **dual-mode binary** that serves as the memory backbone for AI agents working on large codebases. It provides a 5-layer knowledge graph via the Model Context Protocol (MCP), integrating with Cursor, Cline, Claude Desktop, and Zed.

---

## One-time Global Setup (Do this once per machine)

fog-context uses a **single universal binary** at `~/.fog/bin/fog-mcp-server` shared across all your repos. You only install once — every project just points to the same binary.

### Step 1: Download the binary

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

After install, your fog home directory looks like:

```
~/.fog/
├── bin/
│   └── fog-mcp-server     ← Universal binary (shared by all repos)
└── registry.json          ← Auto-created. Tracks all indexed repos.
```

### Step 2: Configure your AI editor (one-time, works for ALL repos)

The key insight: use `${workspaceFolder}` so the same config auto-adapts to whichever repo you open.

#### Cline / Claude Desktop (`cline_mcp_settings.json` or `claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "fog-context": {
      "command": "/home/your-username/.fog/bin/fog-mcp-server",
      "args": ["--project", "${workspaceFolder}"]
    }
  }
}
```

> Replace `/home/your-username` with your actual home directory path.

#### Cursor (`.cursor/mcp.json` at your home or workspace root)

```json
{
  "mcpServers": {
    "fog-context": {
      "command": "${env:HOME}/.fog/bin/fog-mcp-server",
      "args": ["--project", "${workspaceFolder}"]
    }
  }
}
```

#### Zed (`~/.config/zed/settings.json`)

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

---

## Adding a New Repo (What AI Agents Should Do)

When you open a new project in your IDE, fog-context is already running (binary is already configured). The AI agent needs to initialize the index for this specific repo.

### For AI Agents: Protocol when starting on a new repo

```
1. Call fog_brief({})
   → If symbols = 0 or "DB not found": this is a fresh repo, run fog_scan

2. Call fog_scan({})
   → fog-context auto-creates .fog-context/context.db in the project root
   → Auto-registers the project in ~/.fog/registry.json
   → Auto-generates .fog-context/AGENTS.md with tool instructions
   → Returns: file count, symbol count, any parser warnings

3. Verify with fog_roots({})
   → Should now show this project in the list

4. Build knowledge layers (mandatory for full intelligence):
   → fog_assign({ domain: "Authentication", symbols: ["login", "auth_check"] })
   → fog_constraints({ path: "." })    ← scans for ADR files
   → fog_decisions({ functions: ["key_fn"], reason: "why it works this way" })
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
│   └── AGENTS.md                  ← Auto-generated agent instructions
└── .fog.yml (optional)            ← Custom config (ADR paths, ignore patterns)
```

### Optional: `.fog.yml` per-project config

If your project stores ADRs somewhere other than `logs/decisions/`:

```yaml
# .fog.yml — fog-context project config (optional)
adr_paths:
  - docs/decisions
  - docs/adr
  - .agent/decisions
  - logs/decisions
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

# With mobile language support (Kotlin/Swift/Dart — requires native C toolchain)
cargo build --release --package fog-mcp-server --features all-langs
```

---

## 14 MCP Tools

| Tool | Purpose | Priority |
|:---|:---|:---|
| `fog_brief` | Index health check — **call first every session** | 🔴 Mandatory |
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
| `fog_constraints` | Ingest ADR files as architecture constraints | Advanced |
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

## CI/CD Integration

The GitHub Actions workflow at `.github/workflows/release.yml` automatically builds binaries for all platforms when a version tag is pushed:

```bash
git tag v0.5.4
git push origin --tags   # Triggers release build for Linux/macOS/Windows
```

---

## License

MIT — See [LICENSE](LICENSE)
