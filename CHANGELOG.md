# Changelog

All notable changes to fog-context will be documented in this file.

Format: [Semantic Versioning](https://semver.org). Entries grouped by type:
`Added` / `Fixed` / `Changed` / `Removed`.

---

## [0.6.1] - 2026-04-20

### Added

- **Hybrid Hint System** - Two-tier semantic bridge detection:
  - **Tier 1 (Built-in Bridge Queries):** Java `@Autowired/@Inject/@Bean` -> `DI_INJECT` edges; Python decorators -> `DECORATES`; TS/JS `import()`/`require()` -> `DYNAMIC_IMPORT`. Zero config.
  - **Tier 2 (JSON Hint Files):** `.fog-context/hints/{lang}.json` for C macros, custom DI frameworks. Template auto-generated on first `fog_scan`.
  - `.gitignore` injection adds `!.fog-context/hints/` exception so hint files are versioned with code.
- **`LangConfig.bridge_query`** - New field for per-language AST bridge queries + `bridge_edge_kind` for edge type.
- **AGENTS.md s6 Multi-Agent:** startup sequence, fail-loud behavior, 6-tier fuzzy matching documentation.
- **AGENTS.md s7 Hint Files:** priority guidance (L2/L3/L4 first), file format examples, HINT_ vs hints/ decision matrix.
- **Batch Commit Pass 1 (#13):** Commits every 500 files. Large repos survive interruption without full rollback.
- **WAL Checkpoint:** `PRAGMA wal_checkpoint(PASSIVE)` after Pass 2 replaces incorrect G2 evict approach.
- **6-Tier Fuzzy Registry Lookup (G3):** UUID -> path exact -> name exact -> path suffix -> name-contains -> path-segment. Fuzzy tiers reject ambiguous matches with ESCALATE.
- **Fail-Loud Routing (G1):** Unknown `project` -> `ESCALATE_MISSING_CONTEXT` with known-projects list. `fog_scan`/`fog_brief` excepted for first-time indexing.
- **`fog_roots` G4-lite:** Table output with fog_id prefix, symbol count, default project marker, multi-project routing tip.

### Changed

- **`Deferred.edge_kind`:** Cross-file edge dedup key includes edge_kind; DI_INJECT and CALLS edges between same pair both preserved.
- **FTS rebuild timing:** Once after all batches complete, preventing partial FTS state on interrupt.

### Fixed

- **G3 fuzzy false positives:** Tiers 5/6 return None when multiple projects match, preventing silent project misdirection.

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
- **`fog_constraints` inline mode** — push-based Layer 3 constraint injection. AI agents can insert architecture rules directly into the DB without needing ADR files by providing `code` and `statement` directly. It complements the file scanning mode.
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

## [0.5.8] - 2026-04-19

### Added

- **AGENTS.md** — Mandatory AI session protocol document (Sprint A #5). Defines START/END handoff rules, project routing, HINT_ convention, and tool quick-reference. Agents failing to call `fog_decisions` at session end = failed handoff.
- **DbPool multi-project routing** (Sprint B #1) — The server now maintains a `HashMap<PathBuf, MemoryDb>` connection pool (max 8 slots) keyed by project root. Any tool call with `{ "project": "cashew" }` automatically switches to the correct DB without restarting the server. Backward-compatible: no `project` arg → uses default.
- **Git-based stale detection** (Sprint B #3) — `fog_inspect` and `fog_impact` now warn when the target file has changed since the last `fog_scan`. Uses 3-tier fallback: (1) `git log --since=<last_indexed>` + `git status --porcelain`, (2) `stat(file).mtime`, (3) silent Unknown. Warning prepended to output with `[!WARNING]` callout.
- **`fog_scan` Up-to-date message** (Sprint A #12b) — When no files changed, returns a clear "✅ Up-to-date — No changes detected" message with total symbol count instead of ambiguous "0 indexed" stats that looked like a failure.
- **Symbol collision disambiguation in `fog_inspect`** (Sprint A #8) — When multiple symbols share the same name, returns all candidate locations and asks AI to re-call with `{ "file": "<path>" }` hint. Re-routes to `context_symbol_with_file()` for `WHERE path LIKE '%hint%'` precision.
- **Grammar warnings in `fog_brief`** (Sprint A #15) — Parse errors from the last `fog_scan` are persisted in `registry.json` (`grammar_warnings` field) and surfaced in `fog_brief` output under "Grammar Warnings" section. Agents see broken grammars immediately on session start.
- **Fuzzy path matching in `fog_outline`** (Sprint B #9) — When exact/prefix path returns 0 symbols, falls back to `WHERE f.path LIKE '%path%'` (suffix/partial) search. Returns results with fuzzy-match notice. Implements new `skeleton_fuzzy()` in `fog-memory`.
- **Raw text file as macro constraint** (Sprint B #10) — `fog_constraints({ "path": "some/file.txt" })` now ingests a single file (`.clinerules`, `hints.yaml`, `invariants.txt`, etc.) as bulk constraints. Supports plain text, `HINT_NAME: statement` format, and `CODE:SEVERITY:statement` triple format.
- **HINT_ semantic bridge convention** (Sprint B #3L3) — `fog_constraints({ "code": "HINT_API_users", "statement": "..." })` stores cross-language/runtime edges in Layer 3. `fog_inspect` surfaces HINT_ entries with 💡 prefix. Raw text ingestion auto-detects `HINT_*:` prefix.

### Fixed

- **`.gitignore` auto-update** (Sprint A #7) — When `fog_scan` creates `.fog-context/` for the first time, automatically appends `.fog-context/` to the project's `.gitignore` (if present). Prevents accidental commit of 50MB+ SQLite files. Idempotent.
- **Schema mismatch fail-loud** (Sprint A #2) — `create_or_open_db()` now returns a clear actionable error when `schema_version` mismatches: `"SCHEMA INCOMPATIBLE: expected v0.4.0, found vX.Y.Z. Action: rm -f .fog-context/context.db then call fog_scan"`. Previously silently swallowed or showed an opaque rusqlite error.
- **`fog_impact` MAX_NODES cap** (Sprint B #4) — Upstream and downstream result lists are now capped at 100 nodes each. When truncated, appends `[!WARNING]` callout showing N/Total and suggesting `depth=1` to narrow scope. Replaces previous hard-coded `take(20)` limit.

---

## [0.5.7] - 2026-04-19

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
