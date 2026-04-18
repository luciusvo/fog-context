# fog-context — Agentic Codebase Intelligence Engine

> **v0.5.0 — Rust Rewrite** | Zero runtime dependency | <5ms cold start | 14 MCP Tools

fog-context is a **dual-mode binary** that serves as the memory backbone for AI agents working on large codebases. It provides a 5-layer knowledge graph via the Model Context Protocol (MCP), integrating with Cursor, Cline, Claude Desktop, and Zed.

---

## Quick Start

### Option A — Download Binary (Recommended)
Download the prebuilt binary for your platform from the [GitHub Releases](../../releases) page:

| Platform | File |
|:---|:---|
| Linux (x86_64) | `fog-mcp-linux-amd64` |
| macOS (Apple Silicon) | `fog-mcp-macos-arm64` |
| Windows (x86_64) | `fog-mcp-windows-amd64.exe` |

### Option B — Build from Source
```bash
# Minimum Rust version: 1.75+
cd IDE
cargo build --release --package fog-mcp-server

# With mobile language support (Kotlin/Swift/Dart)
cargo build --release --package fog-mcp-server --features all-langs
```

---

## Connect to Your AI Editor

### Cline / Claude Desktop
Add to your `cline_mcp_settings.json` or `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "fog-context": {
      "command": "/absolute/path/to/fog-mcp-server",
      "args": ["--project", "/absolute/path/to/your/codebase"]
    }
  }
}
```

### Cursor
Add to `.cursor/mcp.json`:
```json
{
  "mcpServers": {
    "fog-context": {
      "command": "/absolute/path/to/fog-mcp-server",
      "args": ["--project", "${workspaceFolder}"]
    }
  }
}
```

---

## CLI Mode (for Humans & CI Pipelines)

The same binary exposes human-friendly subcommands:

```bash
# Index a project (incremental by default)
fog-mcp-server index --project /path/to/project

# Force full re-index
fog-mcp-server index --full

# Print index statistics
fog-mcp-server stats

# Export knowledge snapshot (xml | md | json)
fog-mcp-server export --format xml

# List all 14 available MCP tools
fog-mcp-server --list-tools
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
| `fog_roots` | List all indexed projects | Core |
| `fog_gaps` | Find orphans, cycles, dead code | Advanced |
| `fog_domains` | Query business domains and their symbols | Advanced |
| `fog_assign` | Define/update a business domain | Advanced |
| `fog_constraints` | Ingest ADR files as architecture constraints | Advanced |
| `fog_decisions` | Record WHY code was changed (builds causality log) | Advanced |
| `fog_import` | Migrate from ByteRover / GitNexus to fog-context | Advanced |

---

## Agent Workflow (Mandatory 4-Step Protocol)

Every AI agent session **must** follow this order:

```
1. fog_brief()         → Verify index is fresh (symbols > 0)
2. fog_lookup/domains  → Orient to the codebase
3. fog_impact(target)  → Check blast radius BEFORE any edit
                         → risk=HIGH/CRITICAL? STOP and warn user
4. fog_decisions(...)  → Record WHY after every significant change
```

---

## Language Support

| Language | Extensions | Notes |
|:---|:---|:---|
| Rust | `.rs` | Native via `tree-sitter-rust` crate |
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

The GitHub Actions workflow at `.github/workflows/release.yml` automatically builds binaries for all 3 platforms when a version tag is pushed:

```bash
git tag v0.5.1
git push origin --tags   # Triggers release build
```

---

## Legacy TypeScript Version

The original TypeScript implementation (v0.4.0) is archived at `MCP/fog-context-repo/`.
See `MCP/fog-context-repo/ARCHIVED.md` for migration notes.

---

## License

MIT — See [LICENSE](LICENSE)
