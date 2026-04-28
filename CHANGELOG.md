# Changelog

All notable changes to fog-context will be documented in this file.

Format: [Semantic Versioning](https://semver.org). Entries grouped by type:
`Added` / `Fixed` / `Changed` / `Removed`.

---

## [0.8.1] - 2026-04-29

Patch release: UX/DX hardening across `fog_brief`, `fog_scan`, and documentation (Sprint 6).

### Fixed

- **`fog_brief` — Semantic Search warning (BUG-A1):** Replaced broken `fog-context.md#semantic-setup` link with live HuggingFace URLs. Added missing `tokenizer.json` curl command (required alongside `.onnx`). Reformatted using `[!WARNING]` for IDE visibility.
- **`fog_brief` — REQUIRED ACTIONS separator (BUG-A2):** Added `---` horizontal rule before the `🔴 REQUIRED ACTIONS` block to visually separate it from project stats.
- **`fog_brief` — Duplicate "run fog_scan" message (BUG-A3):** Removed redundant `unindexed_advisory` string that appeared at the bottom of output when project was not yet indexed (duplicated the `version_banner` at the top).
- **`fog_brief` — Binary variant label (BUG-A4):** Output now explicitly labels the running build as `Normal Variant` or `Embedded Variant` under the Semantic Search section, allowing AI Agents to detect installation state and skip redundant download questions.
- **`fog_scan` — CLI redirect threshold (BUG-B1):** Lowered early-exit threshold from >1000 to >500 files to prevent MCP timeout on mid-sized repos (most IDE MCP clients time out around 90–120 seconds).
- **`fog_scan` — Large repo warning threshold (BUG-B2):** Lowered post-scan CLI suggestion threshold from >1000 to >500 files (consistent with B1).
- **`fog_scan` — CLI redirect missing next steps (BUG-B3):** CLI redirect message now includes guidance to populate knowledge layers (fog_assign, fog_constraints, fog_decisions) after CLI indexing completes.
- **`README.md` — AI Agent setup prompt (BUG-C1):** Rewrote the Quick Start AI prompt with explicit conditional logic: Agent calls `fog_brief({})` first — if it responds, binary is already installed (skip download entirely); if tool is unavailable, ask user for Normal vs Embedded preference.

---

## [0.8.0] - 2026-04-26


Major feature release: Semantic Search dual-build, raw text search, and cross-language AST bridge hardening.

### Added

- **`fog_search` (Tool #15)** — Raw text and regex search across all project files. Key features:
  - Context lines (`context_lines: 0–10`) — returns N lines above/below each match, reducing follow-up `view_file` calls
  - Distribution summary — when results exceed 50, groups matches by directory instead of flooding context window
  - File size guard — skips files >1MB and minified bundles (`.min.js`, `.min.css`, `.map`, `.svg`)
  - Path sandbox — `canonicalize` + `starts_with(project_root)` prevents path traversal attacks
  - Extension filter — `includes: ["*.rs", "*.ts"]` support via `ignore::OverrideBuilder`
  - `.gitignore` + `.fogignore` aware — reuses existing walker configuration
  - Stateless — no DB required, reads file system directly at query time (always fresh, pre-scan not needed)

- **Semantic Search Dual-Build (`--features embedding`)** — Feature-flagged ONNX embedding pipeline:
  - Hybrid re-ranking: `0.6 × BM25 + 0.4 × cosine_similarity` in `fog_lookup`
  - Lazy model loading via `OnceLock<Option<SemanticModel>>` — preserves `<5ms` cold start guardrail
  - Raw inference stack: `ort 2.0.0-rc.12` + `tokenizers 0.15` + `ndarray 0.17` (no auto-download)
  - Mean pooling + L2 normalization for `all-MiniLM-L6-v2-q8.onnx`
  - Incremental embedding via `phase_embed` — only embeds symbols not yet in `symbol_embeddings` table
  - Graceful fallback to BM25-only if model file missing

- **`symbol_embeddings` DB table** — `CREATE TABLE IF NOT EXISTS` (backward-compatible; non-embed builds read DB without error)

- **`fog_brief` semantic status indicator** — `🔍 Semantic Search: ✅ Active / ⚠️ Model not found / ❌ Disabled` depending on build and model presence

- **CI/CD embed binary variants** — Release matrix now produces 6 standard + 3 embed targets:
  - `fog-mcp-linux-amd64-embed`, `fog-mcp-macos-arm64-embed`, `fog-mcp-windows-amd64-embed.exe`

- **ADR 003** (`logs/decisions/003-semantic-search.md`) — Architectural Decision Record for dual-build strategy

- **Ruby DSL macro bridge** — `[(call) (command_call)]` captures both `has_many(:posts)` and `has_many :posts` patterns

- **Go struct embedding filter** — Excludes type assertion (`x.(Interface)`) from call graph to prevent false-positive edges

- **`fog_scan` Knowledge Layer bootstrap hint** — When L2/L3/L4 are empty, output includes multi-pass population prompt with mid-tier model cost advice

- **`AGENTS.md` `[!WARNING]` banner** — Auto-generated section now prominently warns AI agents not to run `fog_*` commands via bash/shell

### Changed

- **`fog_lookup` scoring** — Fixed hybrid weight from `BM25 × 0.1 + cosine` → `BM25 × 0.6 + cosine × 0.4` (aligns with spec)
- **`fog_scan` large-repo routing** — MCP returns advisory to use CLI when estimated file count > 1000
- **README** — Updated to 15 MCP tools; added `fog_search` to tool table and argument reference; added embed build-from-source command
- **tools/mod.rs** — Updated tool count comment from 14 → 15

### Fixed

- **Embedding build blocker** — Replaced `fastembed 3.x` (which pulled `ort-sys 2.0.0-rc.12` with missing C-FFI `size_t`) with direct `ort` + `tokenizers` + `ndarray` stack. Embedding build now compiles with 0 errors.
- **`is_test_path` dead code warning** — Added `#[allow(dead_code)]`
- **`elapsed_ms` overflow warning** — Fixed cast in scan timing

---

## [0.6.5] - 2026-04-20

Stability hardening for multi-agent workflows. Fixes critical chicken-and-egg
issues in project registration, SQLite locking under concurrent access, and
semantic bridging (AST limits bypassed via Hint Sync / Alias Mapping).

### Fixed

- **Multi-Tenant Hot-Reload Cache** — `handle_tools_call` now eagerly reloads `Registry` on every MCP request to fix `ESCALATE_MISSING_CONTEXT` when a separate CLI process registers a new project. Previously, the static boot `Registry` became a stale cache.
- **Global Error Telemetry Separation** — Parser/Tree-sitter compiler errors (`query_errors`) are no longer saved within `registry.json` (bloating the global multiplexer). Instead, they are cleanly dumped to a strictly separated log file: `~/.fog/logs/parser_errors.log`.
- **Node Classification Syntax in Swift/Lua/C++** — Patched query predicates (`#eq?`) ensuring nodes without valid type tags (e.g. implicitly typed swift `class_declaration`) are resilient against compilation crashes.
- **fog_brief chicken-and-egg routing** — `fog_brief` now immediately registers the
  project in `~/.fog/registry.json` when generating a new `fog_id`. Previously, the
  `fog_id` returned by `fog_brief` could not be used in a subsequent `fog_scan` call
  because the project wasn't in the registry yet, causing `ESCALATE_MISSING_CONTEXT`.
- **CLI registry early registration** — `fog-mcp-server index` now registers the project
  in `~/.fog/registry.json` BEFORE calling `run_scan()`. Previously, if indexing failed or
  took a very long time, `registry.json` was never written, leaving agents unable to route
  calls by fog_id.
- **SQLite PRAGMA ordering: busy_timeout before journal_mode** — `PRAGMA journal_mode = WAL`
  requires an exclusive lock to switch modes. Without `busy_timeout` already set on the
  connection, this switch fails immediately with "database is locked" if another connection
  is open. Fixed by running `PRAGMA busy_timeout = 30000` as a SEPARATE first call in both
  `open_shared_db()` and `run_two_pass()`, so the 30-second retry window covers the WAL
  switch itself.
- **"database is locked" diagnostic guidance** — When CLI index fails with `database is
  locked`, it now prints the `ps` command to check for a concurrent indexer and the
  `stats` command to verify completion. Prevents agents from re-triggering index on an
  already-running process.
- **Large repo advisory in fog_brief** — When a project is not yet indexed, `fog_brief`
  performs a quick file count (excluding `.git/`, `node_modules/`, `target/`, etc.) and
  shows the appropriate action:
  - > 1 000 files: `⚠️ Large project (~N files) — use CLI for initial indexing`
  - ≤ 1 000 files: `📁 ~N files — run fog_scan({ "project": "fog_id" })`
  Agents can decide CLI vs MCP **before** submitting `fog_scan`.
- **README `FOG_PROJECT` removal** — Scenario B ("`FOG_PROJECT` env var") remained in
  README despite the deprecation in v0.6.2, causing agents to set an unsupported option.
  Replaced with correct Scenario A (multi-project, fog_id per-call) and Scenario B
  (single-project, `--project` arg).
- **README binary verification step** — Step 1 now includes explicit `ls -la` and `stats`
  verification commands. Agents can confirm the binary is executable before proceeding.
- **README `registry.json` timing note** — Clarified that `registry.json` is created on
  first `fog_brief` (MCP) or `fog-mcp-server index` (CLI), NOT at install time.

### Added

- **Implicit Bridge / Framework Hint Engine Instructions** — `AGENTS.md` explicitly lists `.fog-context/hints/<lang>.json` (`di_annotations`, `extra_calls`, etc.) under a newly added **Advanced Framework Magic** section so Agents can dynamically bridge missing AST connections globally.
- **Semantic Synonym Injection** — Emphasized `fog_assign({ keywords: ["mail"] })` inside `AGENTS.md` onboarding snippet to proactively encourage Agents to build semantic aliases (without using Vector/RAG mechanisms).
- **AGENTS.md STEP -1** — New mandatory pre-step before fog_id lookup: determines the
  project path from context (single-project mode / task prompt / `fog_roots()` listing).
  Closes the gap where all 3 fog_id paths (A/B/C) assumed the agent already knew the path.

## [0.6.2] - 2026-04-20

Consolidates multi-tenant architecture rewrite: fog_id routing protocol, Hybrid Hint
System, DbPool LRU/TTL management, and IOPS reduction.

### Added

- **Hybrid Hint System** — Two-tier semantic bridge detection:
  - **Tier 1 (Built-in Bridge Queries):** Java `@Autowired/@Inject/@Bean` → `DI_INJECT` edges; Python decorators → `DECORATES`; TS/JS `import()`/`require()` → `DYNAMIC_IMPORT`. Zero config.
  - **Tier 2 (JSON Hint Files):** `.fog-context/hints/{lang}.json` for C macros, custom DI frameworks. Template auto-generated on first `fog_scan`.
  - `.gitignore` injection adds `!.fog-context/hints/` exception so hint files are versioned with code.
- **`LangConfig.bridge_query`** — New field for per-language AST bridge queries + `bridge_edge_kind`.
- **fog_id Lifecycle Hardening** — Solves multi-agent routing contamination:
  - `fog_scan` response: prominent `fog_id` table + "Save this" instruction
  - `fog_brief`: `fog_id` as first visible field + binary version banner
  - `ensure_project_id()`: eager generation at MCP startup (before index runs)
  - fog_id format: `fog_{48-bit ts_ms}{16-bit pid}{16-bit counter}` — globally unique, no external deps
- **Version Check** — `indexer_version` stored in `.fog-context/config.toml` after each scan:
  - `fog_brief` compares binary version vs indexed version → 🆕 banner if stale
  - Agents/users know immediately when a new binary requires a re-scan
- **6-Tier Fuzzy Registry Lookup:** UUID → path exact → name exact → path suffix → name-contains → path-segment.
- **`fog_roots`:** Table output with fog_id, symbol count, default marker, routing tip.
- **Batch Commit Pass 1 (#13):** Commits every 500 files. Large repos survive interruption.
- **WAL Checkpoint:** `PRAGMA wal_checkpoint(PASSIVE)` after Pass 2.
- **fog_scan Large Repo Advisory:** > 1000 files → warns + recommends CLI indexing.
- **AGENTS.md writer:** Generates `<!-- fog-context -->` ... `<!-- /fog-context -->` section with fog_id protocol, version check guidance, and large-repo CLI notes.
- **`fog_constraints` inline mode** — push-based Layer 3 injection without ADR files.
- **`fog_constraints` init mode** — `fog_constraints({ "init": true })` bootstraps Layer 3.
- **CLI indexing progress output** — `eprintln` progress markers on stderr every 500-file batch.

### Changed

- **`FOG_PROJECT` env var REMOVED** — caused process-global contamination in multi-agent setups.
  - Replaced by per-request `{ "project": "fog_id" }` routing.
  - `resolve_project_root()` → `resolve_project_root_opt()` returning `Option<PathBuf>`.
- **`DbPool` complete rewrite:**
  - `default_root: Option<PathBuf>` — `None` = multi-project mode, no implicit default.
  - **LRU eviction** via `VecDeque` — evicts least-recently-used (not arbitrary first-found).
  - **Idle TTL** — connections unused > 10 min closed on next `gc_idle()` (no background thread).
  - **`max_open = 4`** (was 8) — each connection uses ~2MB SQLite page cache.
  - **Post-scan eviction** — after `fog_scan` completes, write connection evicted → next query opens fresh reader.
- **SQLite PRAGMAs** — `cache_size = -2000` (2MB, was 8MB) + `mmap_size = 0` (disables mmap) per connection.
- **FD inheritance fix** — MCP server calls `close_range(3, MAX)` at startup to close all file descriptors inherited from Electron parent (LevelDB, GPU cache handles).
- **`Deferred.edge_kind`:** Cross-file edge dedup includes edge_kind; DI_INJECT and CALLS between same pair both preserved.

### Fixed

- **G3 fuzzy false positives:** Tiers 5/6 return None when multiple projects match.
- **Fail-loud routing:** Unknown `project` → `ESCALATE_MISSING_CONTEXT` with known-projects list.
- **`ensure_project_id()`** calls `ProjectConfig::load()` to preserve existing `name`/`adr_paths`.
- **`fog_brief`** now uses `ensure_project_id()` (not `read_project_id`) so fog_id always present.
- **Multi-project ESCALATE_NO_DEFAULT_PROJECT:** Calls without `project` arg in multi-project mode return actionable error with instructions.

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

---

## [0.5.0]

Initial release — TypeScript prototype.

- 8 core MCP tools
- Tree-sitter AST parsing for 5 languages
- SQLite-backed knowledge graph
- Basic project registry
