# Changelog

All notable changes to fog-context will be documented in this file.

Format: [Semantic Versioning](https://semver.org). Entries grouped by type:
`Added` / `Fixed` / `Changed` / `Removed`.

---

## [0.5.7] - 2026-04-19

### Added

- **Smart project root resolution** (priority chain):
  - Auto-detect if CWD has `.fog-context/` directory
  - Walk up ancestor directories to find `.fog-context/`
  - `FOG_PROJECT` env var — reliable path for headless agents and CI
  - CWD fallback with a clear warning log (no more silent wrong-project)
- **Project config file** `.fog-context/config.toml` — unifies `.fog-id` (old) and `.fog.yml` into a single TOML file storing `fog_id`, project name, and ADR paths. Auto-migrates legacy `.fog-id` on first run.
- **`fog_brief` project identity** — output now shows active project name, path, `fog_id`, and DB size. Agents can verify the correct project at session start.
- **`FOG_PROJECT` env var** — MCP config supports `"env": { "FOG_PROJECT": "..." }` for headless setups where CWD is unreliable.
- **`fog_add_constraint`** (Tool #15) — push-based Layer 3 constraint injection. AI agents can insert architecture rules directly into the DB without needing ADR files. Complements `fog_constraints` (file-scan based).
- **`fog_constraints` init mode** — `fog_constraints({ "init": true })` bootstraps Layer 3 by creating `logs/decisions/` and a template ADR file. Idempotent.
- **CLI indexing progress output** — `eprintln` progress markers on `stderr` at key indexer stages (`Walking files → Found N files → Pass 1 done → Pass 2 done → Registry updated`). Visible in CLI mode, invisible to MCP JSON-RPC.

### Changed

- **README Step 2** rewritten with 3 clear scenarios: A (IDE auto CWD), B (headless + env var), C (explicit `--project` lock).

### Fixed

- **Swift grammar query:** Removed invalid `struct_declaration` node type (doesn't exist in tree-sitter-swift). Swift structs are represented as `class_declaration` with `declaration_kind = struct`. Fixes `Query error at 5:2: Invalid node type struct_declaration`.
- **Dart grammar ABI:** Upgraded tree-sitter `0.24 → 0.25` to support ABI version 15. The bundled Dart grammar uses `LANGUAGE_VERSION 15` which was rejected by tree-sitter 0.24 (max ABI 14). Fixes `Incompatible language version 15. Expected minimum 13, maximum 14`.
- **Grammar upgrades:** tree-sitter-rust `0.23→0.24`, tree-sitter-python/go `0.23→0.25` for latest grammar improvements.
- **Registry `symbol_count = 0`** — after an incremental rescan with no changes, the registry no longer overwrites the total count with the delta (0). Now queries `SELECT COUNT(*)` from the DB after every scan.
- **`fog_assign` schema mismatch** — tool now accepts both `domain` and `name` as the primary key parameter. Previously, calling with `{ "domain": "..." }` returned `'name' is required`.
- **`fog_outline({ "path": "." })`** — root path requests now return a helpful redirect message instead of a misleading "No symbols found" error.
- **macOS Intel CI runner** — removed `macos-13` from GitHub Actions matrix (runner was unavailable for 9+ hours). Intel Mac users: build from source or use ARM64 via Rosetta 2.


---

## [0.5.0] - Initial Rust Rewrite

Complete rewrite from TypeScript to Rust. Zero runtime dependencies, <5ms cold start.

### Core (8 tools)

| Tool | Description |
|:-----|:------------|
| `fog_brief` | Index health check — call first every session |
| `fog_scan` | Index or re-index project with Tree-sitter AST |
| `fog_lookup` | Full-text symbol search (BM25 weighted by call-graph) |
| `fog_outline` | Lightweight file outline (names + signatures, no source) |
| `fog_inspect` | 360° symbol context: callers, callees, constraints, decisions |
| `fog_impact` | Blast radius analysis before editing |
| `fog_trace` | Full execution flow trace from an entry point |
| `fog_roots` | List all registered projects in global registry |

### Advanced (6 tools)

| Tool | Description |
|:-----|:------------|
| `fog_gaps` | Find orphans, cycles, dead code in call graph |
| `fog_domains` | Query business domains and their symbols |
| `fog_assign` | Define or update a business domain (Layer 2) |
| `fog_constraints` | Ingest ADR files as architecture constraints (Layer 3) |
| `fog_decisions` | Record WHY code changed (Layer 4 causality) |
| `fog_import` | Migrate from ByteRover / GitNexus to fog-context |

### Language support (12 native grammars)

Rust, TypeScript/TSX, Python, Go, C, C++, Java, C#, Ruby, PHP, Lua

### Optional language support (build with `--features all-langs`)

Kotlin, Swift, Dart
