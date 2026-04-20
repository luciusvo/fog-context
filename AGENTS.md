# fog-context — AI Agent Onboarding Guide (v0.6.2)

> **READ THIS BEFORE TOUCHING ANY CODE.**
> This file tells you exactly how to use fog-context tools, in what order, and most importantly — how to maintain the knowledge layers so future sessions don't start from zero.

---

## 0. MANDATORY: Session Handoff Protocol

This is the most important section. Violating it = **FAILED HANDOFF**.

### STEP 0: Get Your fog_id (before ANY other MCP call)

**Path A — Project already indexed (fastest):**
```bash
# Read directly from filesystem (no MCP call needed)
cat {project_root}/.fog-context/config.toml
# fog_id = "fog_019506a8b3f8..."
```

**Path B — Project already indexed but no shell access:**
```
fog_brief({ "project": "/absolute/path/to/project" })
# First line of response: fog_id: `fog_019506...`  ← capture this
```

**Path C — Project NOT yet indexed (first time):**
```
fog_scan({ "project": "/absolute/path/to/project" })
# Response table: | fog_id | `fog_019506a8b3f8...` |  ← capture this
```

> ⚠️ **Once you have fog_id: use it in EVERY subsequent call.**
> `{ "project": "fog_019506a8b3f8..." }` — this is the only routing key that cannot cause contamination.

### STEP 1: Verify + Load domain map
```
fog_brief({ "project": "<fog_id>" })     → verify correct project, index fresh?
fog_domains({ "project": "<fog_id>" })   → load domain map before touching code
```

### SESSION END (MANDATORY before reporting "done")
```
fog_decisions({ "project": "<fog_id>", "functions": ["fn_a"], "reason": "why" })
```

If you discovered cross-language or runtime bridges (REST, DI, macros), also:
```
fog_constraints({ "project": "<fog_id>", "code": "HINT_<NAME>", "statement": "fetch('/api/users') → UserController.index()" })
```

> ⚠️ **Reporting task complete WITHOUT calling fog_decisions = FAILED HANDOFF.**

---

## 1. Tool Invocation Order

### Step 1 — Orient yourself
```
fog_brief({})                    → check project identity + index freshness
fog_domains({})                  → list known business domains
```

### Step 2 — Find and understand code
```
fog_lookup({ "query": "<concept>" })         → locate symbols by name/type/doc
fog_outline({ "path": "<file or dir>" })     → lightweight outline (names + signatures, no source)
fog_inspect({ "name": "<symbol>" })          → 360° view: callers, callees, constraints, decisions
fog_trace({ "entry": "<fn>", "direction": "down" }) → trace full execution flow
```

### Step 3 — Before ANY edit (mandatory safety check)
```
fog_impact({ "target": "<symbol>", "direction": "both" })
  → risk = HIGH or CRITICAL? STOP — warn the user and get explicit approval first
```

### Step 4 — After completing changes
```
fog_decisions({ "functions": [...], "reason": "...", "revert_risk": "LOW|MEDIUM|HIGH" })
```

### Step 5 — If cross-language / runtime bridges discovered
```
fog_constraints({ "code": "HINT_API_users", "statement": "GET /api/users → UserController.index()" })
fog_constraints({ "code": "HINT_DI_UserRepo", "statement": "@Autowired IUserRepository → UserRepositoryImpl" })
```

---

## 2. Project Routing

fog-context uses **fog_id** as the stable, unique routing key. Pass it with every call in multi-project setups.

**Where to find fog_id:**
```bash
# Option 1: read from file (fastest, no MCP)
cat /path/to/project/.fog-context/config.toml  # fog_id = "fog_..."

# Option 2: from fog_brief response (first line)
fog_brief({ "project": "/absolute/path" })      # → fog_id: `fog_...`

# Option 3: first-time index
fog_scan({ "project": "/absolute/path" })       # → response includes fog_id

# Option 4: list all registered projects  
fog_roots({})                                   # → table with fog_id column
```

**Using fog_id in calls:**
```
fog_brief({ "project": "fog_019506a8b3f8..." })
fog_lookup({ "query": "auth", "project": "fog_019506a8b3f8..." })
```
fog_id accepts: full ID (e.g. `fog_019506a8b3f812a40003`) OR short name OR path suffix.

**Single-project setup (--project at startup):**
```json
{ "fog-context": { "command": "/path/to/fog-mcp-server", "args": ["--project", "/abs/path"] } }
```
In this setup, omitting `project` arg is OK (server has a default).

**Multi-project setup (no --project at startup):**
```json
{ "fog-context": { "command": "/path/to/fog-mcp-server" } }
```
In this setup, **every call MUST include `"project": "<fog_id>"`** or server returns ESCALATE_NO_DEFAULT_PROJECT.

> ⚠️ Do NOT use `FOG_PROJECT` env var — it was removed in v0.6.2 because it caused
> multi-agent routing contamination (process-global state shared across all sessions).

---

## 3. Quick Reference

| Task | Tool |
|:-----|:-----|
| Verify correct project loaded | `fog_brief({})` |
| List all indexed projects | `fog_roots({})` |
| Re-index after code changes | `fog_scan({})` |
| Find a function | `fog_lookup({ "query": "..." })` |
| File/module outline | `fog_outline({ "path": "src/auth" })` |
| Understand a function | `fog_inspect({ "name": "verify_token" })` |
| Trace call flow | `fog_trace({ "entry": "main", "direction": "down" })` |
| What breaks if I change X? | `fog_impact({ "target": "X" })` |
| Dead code / cycles | `fog_gaps({ "template": "find_orphans" })` |
| Business rules for a feature | `fog_domains({})` then `fog_domains({ "domain": "Authentication" })` |
| Add domain + link symbols | `fog_assign({ "name": "Auth", "symbols": ["verify_token"] })` |
| Architecture constraints (ADRs) | `fog_constraints({ "path": "docs/adr" })` |
| Inject inline constraint | `fog_constraints({ "code": "NO_DIRECT_DB", "statement": "..." })` |
| Record semantic hint | `fog_constraints({ "code": "HINT_...", "statement": "..." })` |
| Log a change with reason | `fog_decisions({ "functions": [...], "reason": "..." })` |
| Check grammar parse errors | `fog_brief({})` → Grammar Warnings section |

---

## 4. Anti-Patterns ❌

| ❌ Forbidden | ✅ Correct |
|:-----------|:----------|
| Read source files before calling `fog_inspect` | Call `fog_inspect({ "name": "..." })` first |
| Edit a symbol without calling `fog_impact` | Always check blast radius first |
| Report "done" without `fog_decisions` | Log every significant change |
| Call `fog_scan` repeatedly without code changes | `fog_brief` will tell you if graph is Up-to-date |
| Trust `fog_impact` when files changed externally | `fog_brief` shows staleness warnings |
| Ignore `risk: CRITICAL` warning | Always surface to user first |
| Use `fog_inspect("main")` when multiple files have `main` | Re-call with `{ "name": "main", "file": "src/main.rs" }` |

---

## 5. HINT_ Convention

When static analysis cannot capture a connection, record it explicitly:

```
fog_constraints({ "code": "HINT_<DOMAIN>_<NAME>", "statement": "<description of bridge>" })
```

Examples:
- `HINT_API_users` — REST endpoint bridge between frontend call and backend handler  
- `HINT_DI_UserRepo` — Spring DI binding between interface and implementation
- `HINT_MACRO_LLAMA_API` — C macro expansion: `LLAMA_API` → `llama_api_impl()`

These are stored in Layer 3 (Constraints) and surfaced in `fog_inspect` output.
Best time to record: **SESSION END handoff** after discovering a cross-boundary edge.

---

## 6. Multi-Agent Scenario (concurrent sessions)

When multiple agents work on the same mono-repo or across related projects simultaneously:

### Projects are isolated by design
- Each project has its own `.fog-context/context.db`
- A single MCP process can serve up to **4 projects** concurrently (DbPool with LRU eviction)
- **Always pass `"project"` arg** when running multiple agents

### Startup sequence (each agent)
```
fog_roots({})                            → list all registered projects + default marker 🎯
fog_brief({ "project": "my-project" })  → verify THIS agent is on the right project
fog_domains({ "project": "my-project" }) → load domain map for this project
```

### Fail-loud behavior
If `"project"` value not found in registry → **ESCALATE_MISSING_CONTEXT** error returned.
Do NOT retry blindly. Either:
1. Run `fog_scan({ "project": "/absolute/path" })` to register the project first
2. Check `fog_roots` for the correct name/fog_id

### Fuzzy matching (6-tier)
1. fog_id UUID exact
2. Path string exact
3. Name exact
4. Path suffix
5. Name contains (case-insensitive, **must be unique**)
6. Path last segment contains (case-insensitive, **must be unique**)

→ If tier 5/6 matches >1 project, returns ESCALATE. Use more specific key.

---

## 7. Hint Files — Project-Specific Semantic Bridges

> ⚠️ **Priority:** Record L2 (domains), L3 (constraints), L4 (decisions) FIRST.
> Hint files are supplementary — only needed when the static graph genuinely cannot capture a relationship.

fog-context uses a **two-tier hint system** for cross-cutting semantic bridges:

### Tier 1: Built-in Bridge Queries (automatic, no config)
Detected automatically during `fog_scan`:
- **Java**: `@Autowired`, `@Inject`, `@Resource`, `@Bean` annotations → `DI_INJECT` edges
- **Python**: decorator usage (`@property`, `@router.get`) → `DECORATES` edges  
- **TypeScript/JS**: `import('./module')`, `require('./module')` → `DYNAMIC_IMPORT` edges

No action needed — these appear in `fog_inspect` and `fog_impact` output automatically.

### Tier 2: Per-Language JSON Hint Files (opt-in, project-specific)
For patterns the static AST **cannot** detect (C macros, custom DI, framework-specific wiring):

**Location:** `.fog-context/hints/{lang}.json`  
**Auto-created:** `_template.json` on first `fog_scan`

Example `.fog-context/hints/c.json`:
```json
{
  "macro_expansions": [
    { "pattern": "LLAMA_FUNC", "resolves_to": "llama_func_impl" },
    { "pattern": "MY_HANDLER", "resolves_to": "MyHandlerImpl" }
  ]
}
```

Example `.fog-context/hints/java.json`:
```json
{
  "di_annotations": ["@MyCustomInject", "@Provides"],
  "extra_calls": [
    { "from": "bootstrap", "to": "AppKernel" }
  ]
}
```

**When to use hint files (not HINT_ constraints):**
- C/C++ macros that expand to actual function calls
- Project-specific DI annotations not in the built-in list
- Framework wiring that's not capturable by AST

**When to use HINT_ constraints (not hint files):**
- REST API bridges (frontend call → backend handler)
- Event bus / message queue routing
- Any one-off cross-boundary relationship discovered during session

> **Commit hint files** to keep them versioned with the project:
> Add `!.fog-context/hints/` to `.gitignore` exceptions (fog-context does this automatically).

---

## 8. Version Check — How to Detect a New Binary

When a new `fog-mcp-server` binary is dropped in `~/.fog/bin/`, agents and users need to know.

**Automatic detection via `fog_brief`:**
```
fog_brief({ "project": "<fog_id>" })
```
Response header shows:
```
# fog-context v0.6.3 — Status
## 🔑 Project Identity
...
Indexed by: v0.6.2

> 🆕 New binary detected! Binary: `v0.6.3` | Index built by: `v0.6.2`
> Run fog_scan({ "project": "..." }) to refresh the index with the new version.
```

**Rule:**
- `Indexed by` == binary version → index is current, no action needed
- `Indexed by` < binary version → **run `fog_scan`** to rebuild with new parser/features
- `Indexed by` missing → project never indexed → **run `fog_scan`** first

**CLI alternative:**
```bash
fog-mcp-server index --project /path/to/project
```
Outputs the binary version used and rebuilds the index.
