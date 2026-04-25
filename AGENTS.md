<!-- fog-context -->
## fog-context MCP - Agent Instructions
> Auto-generated 2026-04-25T22:50:35Z | 127 files · 98 symbols · indexed in 764ms

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
<!-- /fog-context -->