<!-- fog-context -->
## fog-context MCP - Agent Instructions

> [!WARNING]
> **⚠️ ALL `fog_*` commands are MCP Tools.** Do NOT run them via bash/shell.

### MANDATORY PROTOCOL
1. **Call `fog_brief` First:** Always check index health.
2. **Before Editing:** Run `fog_impact({ "target": "<symbol>" })` to check blast radius.
3. **After Editing:** Run `fog_decisions` to log WHY the code was changed.

### MCP Tools
- **Orient:** `fog_domains`, `fog_lookup`
- **Understand:** `fog_inspect`, `fog_trace`
- **Verify:** `fog_impact`
- **Record:** `fog_decisions`

⚠️ **Anti-Blackbox Rule:** You MUST NOT bypass cross-validation. 
Even with a detailed prompt, ALWAYS verify real codebase state via `fog_inspect` before modifying code.

### 🔴 First-time Setup — MANDATORY Knowledge Layer Bootstrap
> fog-context indexed Layer 1 (Physical: 152 symbols). Semantic Layers 2-4 are empty — Knowledge Score: 0/100.
> Complete these steps **once** to unlock full intelligence:

```
Step 1 - Layer 2 (Business Domains): fog_assign
Step 2 - Layer 3 (Constraints): fog_constraints
Step 3 - Layer 4 (Decisions): fog_decisions
```
<!-- /fog-context -->