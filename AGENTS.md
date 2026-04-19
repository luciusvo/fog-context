# fog-context — AI Agent Onboarding Guide (v0.6.1)

> **READ THIS BEFORE TOUCHING ANY CODE.**
> This file tells you exactly how to use fog-context tools, in what order, and most importantly — how to maintain the knowledge layers so future sessions don't start from zero.

---

## 0. MANDATORY: Session Handoff Protocol

This is the most important section. Violating it = **FAILED HANDOFF**.

### SESSION START (run BOTH, in order)
```
fog_brief({})       → verify active project name, path, fog_id, DB size
                      If project is wrong: call with { "project": "<fog_id or name>" }
fog_domains({})     → load domain map before touching any code
```

### SESSION END (MANDATORY before reporting "done")
```
fog_decisions({ "functions": ["fn_a", "fn_b"], "reason": "why this changed" })
```

If you discovered any cross-language or runtime binding that the static graph cannot capture (e.g. REST API calls, DI wiring, macro expansions), ALSO record:
```
fog_constraints({ "code": "HINT_<NAME>", "statement": "fetch('/api/users') in frontend → UserController.index()" })
```

> ⚠️ **Reporting task complete WITHOUT calling fog_decisions = FAILED HANDOFF.**
> This is non-negotiable. The knowledge graph degrades every time this is skipped.

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

fog-context supports multiple registered projects. To work with a specific project:

**Option A — Pass `project` arg to any tool:**
```
fog_brief({ "project": "cashew" })
fog_lookup({ "query": "auth", "project": "cashew" })
```
`project` accepts: fog_id (UUID), project name, or path suffix. Resolved from `~/.fog/registry.json`.

**Option B — environment variable (for IDE/CI config):**
```json
{ "env": { "FOG_PROJECT": "/path/to/project" } }
```

**Option C — explicit `--project` flag (single-project setups):**
```json
{ "args": ["--project", "/path/to/project"] }
```

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
- A single MCP process can serve up to 8 projects concurrently (DbPool)
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
