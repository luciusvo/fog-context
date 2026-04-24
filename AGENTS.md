<!-- fog-context -->
## fog-context MCP - Agent Instructions
> Auto-generated 2026-04-20T21:18:23Z | 123 files · 810 symbols · indexed in 117945ms

### MANDATORY: Start every session

**Step 0 — Get fog_id (before any other call):**
```bash
# Fastest: read from file
cat .fog-context/config.toml   # fog_id = "fog_..."

# Or via MCP:
fog_brief({ "project": "/absolute/path" })   # → fog_id shown at top
```

**Step 1 — Verify project + load domain map:**
```
fog_brief({ "project": "<fog_id>" })   → verify project, check version
fog_domains({ "project": "<fog_id>" }) → load business domain map
```

### Tool Order
1. **Orient:** fog_domains → fog_lookup
2. **Understand:** fog_inspect → fog_trace
3. **Before edit:** fog_impact (HIGH/CRITICAL → warn user first)
4. **After edit:** fog_decisions { functions, reason, revert_risk } (**MANDATORY**)

### Version Check
`fog_brief` shows: `Indexed by: v0.6.x`
If binary version ≠ indexed version → 🆕 banner appears → run fog_scan to refresh.

### 🪄 Advanced: Handling Framework Magic (IoC, Event Bus, Metaprogramming)
If `fog_trace` breaks because of Dependency Injection (Interfaces), Event Buses, or dynamic metaprogramming (like Rails `has_many`), AST cannot see the link. You MUST bridge it via hints:
1. Create `.fog-context/hints/<lang>.json` (e.g. `csharp.json`, `ruby.json`)
2. Use `extra_calls` to bridge EventBus/IoC: `[ { "from": "IUserRepository.Add", "to": "UserRepository.Add" } ]`
3. Use `di_annotations` to tag IoC: `[ "@Inject", "@MyService" ]`
4. Use `macro_expansions` to bridge metaprogramming aliases.
5. Run `fog_scan(full=false)` immediately to apply the hints to the Graph!

### Large Repos (>1000 files)
Prefer CLI for initial index (shows progress, no MCP timeout):
```bash
fog-mcp-server index --project /path/to/project
```
Then use MCP tools for all queries.
### 🔴 First-time Setup - MANDATORY Knowledge Layer Bootstrap
> fog-context auto-indexed Layer 1 (Physical: 810 symbols). Semantic Layers 2-4 are currently empty.
> Complete these steps **once** to unlock full intelligence:

```
Step 1 - Layer 2 (Business Domains): Map Semantic Synonyms to Exact Code
fog_assign({ domain: "Notification", keywords: ["mail", "sms"], symbols: ["NotificationDispatcher"] })
fog_assign({ domain: "DataAccess",   keywords: ["sql", "db"],   symbols: ["db_query"] })

Step 2 - Layer 3 (Constraints): Ingest architecture rules from ADR files
fog_constraints({})          ← scans logs/decisions/, docs/adr/, docs/decisions/

Step 3 - Layer 4 (Decisions): Record WHY key design decisions were made
fog_decisions({ functions: ["key_fn"], reason: "...", revert_risk: "LOW" })
```

### 🔴 MANDATORY: After Every Significant Change
```
fog_decisions({ functions: ["changed_fn"], reason: "WHY it changed", revert_risk: "LOW|MEDIUM|HIGH" })
```
> Completing a task without recording WHY = **KNOWLEDGE GAP VIOLATION**.
> Context Maturity visualization will update as you populate Layers 2-4.

<!-- /fog-context -->