# fog-context v0.9.1 — Patch Release Plan

> Tổng kết từ thảo luận phân tích v1 → v2 → cross-review.  
> Scope: Bug fixes, scoring improvements, documentation. Không thay đổi kiến trúc lớn.

---

## Nguyên tắc

- **Chỉ sửa những gì gây hại thực tế** cho workflow hiện tại (1 developer + 1 AI agent, local)
- **Không over-engineer** — mỗi fix phải justify bằng user impact cụ thể
- **Defer thiết kế v2** — ghi lại nhưng không implement

---

## 1. Fix: Loại bỏ L5 khỏi Knowledge Model

**File:** `crates/fog-memory/src/query.rs`  
**Vấn đề:** Scratchpad (L5) là agent operational state, không phải code knowledge. Nó nằm trong `knowledge_score()` nhưng không có MCP tool nào ghi vào → score max = 75/100, không bao giờ đạt 100.  
**Impact:** Agent thấy score misleading.

**Thay đổi:**

```diff
# query.rs — knowledge_score()
- // Layer 2-5 score: 0-100
+ // Layer 2-4 score: 0-75
  let l2 = if total_domains > 0 { 25u8 } else { 0 };
  let l3 = if total_constraints > 0 { 25u8 } else { 0 };
  let l4 = if total_decisions > 0 { 25u8 } else { 0 };
- let l5: u8 = if count("SELECT COUNT(*) FROM scratchpad") > 0 { 25 } else { 0 };
- let layer_score = l2 + l3 + l4 + l5;
+ let layer_score = l2 + l3 + l4;
```

```diff
# write.rs — doc comments
- //!   scratchpad_get()   → Layer 5 task state read
- //!   scratchpad_update()→ Layer 5 task state write
+ //!   scratchpad_get()   → Agent Runtime: task state read
+ //!   scratchpad_update()→ Agent Runtime: task state write
```

```diff
# brief.rs — fog_brief output
  # Hiện tại hiển thị L4, KHÔNG hiển thị L5 trong Layers Populated
  # (L5 không cần thay đổi vì fog_brief chưa hiện L5 bar riêng)
```

**Giữ nguyên:** Table `scratchpad`, functions `scratchpad_get/update`, tests. Chỉ reclassify.

---

## 2. Fix: symbol_tags Layer Documentation

**File:** `crates/fog-memory/src/db.rs`  
**Vấn đề:** `symbol_tags` lưu policy metadata (SOURCE/SINK/PII) nhưng nằm cùng schema với L1 physical data. Conceptually L3, technically L1.  
**Impact:** Confusion khi đọc schema.

**Thay đổi:**

```diff
# db.rs — SCHEMA_SQL
+ -- symbol_tags: Policy metadata (conceptually L3) stored at L1 level
+ -- for query performance. Tags represent security/compliance annotations,
+ -- not physical code facts. Populated by fog_overlay from .fog-context/security.toml
  CREATE TABLE IF NOT EXISTS symbol_tags (
```

---

## 3. Fix: Domain Drift Detection (Orphaned Mappings)

**File:** `crates/fog-mcp-server/src/tools/brief.rs`  
**Vấn đề:** Sau `fog_scan`, symbols có thể bị xóa/rename nhưng `domain_symbols` giữ mapping cũ. Không có warning.  
**Impact:** L2 domain data stale mà không ai biết.

**Thay đổi:** Thêm orphan check vào `fog_brief`:

```rust
// Sau knowledge score section, trước Session Protocol
let orphan_count: i64 = db.conn().query_row(
    "SELECT COUNT(*) FROM domain_symbols ds
     LEFT JOIN symbols s ON s.name = ds.symbol_name
     WHERE s.id IS NULL",
    [], |r| r.get(0),
).unwrap_or(0);

if orphan_count > 0 {
    lines.push(format!(
        "\n## ⚠️ Domain Drift\n\
         - **{orphan_count} orphaned mappings** — symbols removed from code but still in domains\n\
         - Run `fog_assign` to update affected domains"
    ));
}
```

---

## 4. Fix: Knowledge Score Hint — Outdated Tool Names

**File:** `crates/fog-memory/src/query.rs`  
**Vấn đề:** `_agent_hint` message references old tool names (`define_domain`, `record_decision`, `ingest_adrs`).  
**Impact:** Agent gọi tools không tồn tại.

**Thay đổi:**

```diff
- "Knowledge score: {layer_score}/100. Run define_domain, record_decision, and ingest_adrs to improve."
+ "Knowledge score: {layer_score}/75. Use fog_assign (L2), fog_constraints (L3), fog_decisions (L4) to improve."
```

---

## 5. Fix: REQUIRED ACTIONS Warning Quá Lenient

**File:** `crates/fog-mcp-server/src/tools/brief.rs`  
**Vấn đề:** Hiện tại REQUIRED ACTIONS chỉ hiện khi `total_domains == 0 || total_constraints == 0`. Nếu có 1 domain bất kỳ → warning biến mất, dù 99% codebase chưa mapped.  
**Impact:** Agent cảm thấy "đã OK" khi chưa OK.

**Thay đổi:** Thêm coverage awareness:

```rust
// Thay thế binary check
if score.total_symbols > 0 {
    let domain_symbol_count: i64 = db.conn().query_row(
        "SELECT COUNT(DISTINCT symbol_name) FROM domain_symbols",
        [], |r| r.get(0),
    ).unwrap_or(0);

    let coverage_pct = if score.total_symbols > 0 {
        (domain_symbol_count as f64 / score.total_symbols as f64 * 100.0) as u8
    } else { 0 };

    if coverage_pct < 30 {
        lines.push(format!(
            "\n## 🔴 LOW COVERAGE\n\
             - L2 Domain coverage: **{}%** ({}/{} symbols mapped)\n\
             - Use `fog_assign` to map more symbols to business domains",
            coverage_pct, domain_symbol_count, score.total_symbols
        ));
    }
}
```

---

## 6. Documentation: Layer Model Clarification

**File:** `crates/fog-memory/src/lib.rs` (module doc), AGENTS.md string  
**Vấn đề:** Doc references "5 layers" nhưng model là 4 layers + Agent Runtime.

**Thay đổi:**

```diff
- //! 5-Layer Knowledge Architecture:
- //! L1 (Physical) → L2 (Business) → L3 (Rules) → L4 (Causality) → L5 (Runtime)
+ //! 4-Layer Knowledge Graph + Agent Runtime:
+ //! L1 (Physical) → L2 (Business) → L3 (Rules) → L4 (Causality)
+ //! Agent Runtime: Scratchpad (operational state, not code knowledge)
```

---

## 7. Feature: L3 Machine-Checkable Rules (DSL)

**Files:** `crates/fog-memory/src/db.rs`, `crates/fog-mcp-server/src/tools/constraints.rs`, new `crates/fog-mcp-server/src/checker.rs`  
**Vấn đề:** Constraints hiện lưu text tự do. Hệ thống biết rule tồn tại nhưng không validate tự động.

**Thay đổi schema:**

```sql
-- Thêm cột type vào constraints table
ALTER TABLE constraints ADD COLUMN rule_type TEXT DEFAULT 'text';
-- rule_type: 'text' | 'forbidden_edge' | 'tag_required'
ALTER TABLE constraints ADD COLUMN rule_config TEXT; -- JSON config cho DSL rules
```

**2 loại rule machine-checkable (KHÔNG implement metric — cần AST pass riêng):**

```yaml
# Loại 1: forbidden_edge — check từ edges table
constraint:
  id: NO_DIRECT_DB
  type: forbidden_edge
  from_tag: "http_layer"   # symbol có tag này
  to_tag: "db_layer"       # không được gọi symbol có tag này
  severity: ERROR

# Loại 2: tag_required — check từ symbol_tags table
constraint:
  id: PII_MUST_BE_AUDITED
  type: tag_required
  has_tag: "pii"
  must_also_have: "audited"
  severity: ERROR
```

**New file `checker.rs`:**

```rust
// Chạy sau fog_scan, hoặc khi fog_constraints được gọi
pub fn run_auto_checks(db: &MemoryDb) -> Vec<Violation> {
    let mut violations = vec![];
    // Query constraints với rule_type != 'text'
    // Với forbidden_edge: SELECT edges WHERE source có from_tag AND target có to_tag
    // Với tag_required: SELECT symbol_tags WHERE has_tag X AND NOT has_tag Y
    violations
}
```

**Output:** Violations hiện trong `fog_brief` nếu có, giống orphan warning.

---

## 8. Feature: L2 Domain Dependency Graph

**File:** `crates/fog-memory/src/db.rs`, `crates/fog-mcp-server/src/indexer/mod.rs`  
**Vấn đề:** `fog_impact` hoạt động ở symbol level. Câu hỏi "Payment domain phụ thuộc bao nhiêu vào Auth domain?" không trả lời được.

**Thay đổi schema:**

```sql
CREATE TABLE IF NOT EXISTS domain_dependencies (
  from_domain  TEXT NOT NULL,
  to_domain    TEXT NOT NULL,
  edge_count   INTEGER DEFAULT 0,  -- tự tính từ L1, không cần human input
  strength     REAL DEFAULT 0.0,   -- edge_count / tổng edges ra ngoài of from_domain
  PRIMARY KEY (from_domain, to_domain)
);
```

**Tính toán sau mỗi `fog_scan`** (thêm vào `indexer/mod.rs`):

```rust
// Sau khi scan xong, tính cross-domain edges
db.conn().execute_batch("
  DELETE FROM domain_dependencies;
  INSERT INTO domain_dependencies (from_domain, to_domain, edge_count)
  SELECT
    d_src.name as from_domain,
    d_tgt.name as to_domain,
    COUNT(*) as edge_count
  FROM edges e
  JOIN symbols s_src ON s_src.id = e.source_id
  JOIN symbols s_tgt ON s_tgt.id = e.target_id
  JOIN domain_symbols ds_src ON ds_src.symbol_name = s_src.name
  JOIN domain_symbols ds_tgt ON ds_tgt.symbol_name = s_tgt.name
  JOIN domains d_src ON d_src.id = ds_src.domain_id
  JOIN domains d_tgt ON d_tgt.id = ds_tgt.domain_id
  WHERE d_src.name != d_tgt.name
  GROUP BY d_src.name, d_tgt.name;
  -- Update strength
  UPDATE domain_dependencies SET strength = (
    CAST(edge_count AS REAL) / (
      SELECT SUM(edge_count) FROM domain_dependencies dd2
      WHERE dd2.from_domain = domain_dependencies.from_domain
    )
  );
")?;
```

**Output:** `fog_domains` hiện thêm dependency section. `fog_brief` cảnh báo nếu có circular domain dependency.

---

## 9. Feature: Bootstrap Auto-Suggest Domains

**File:** `crates/fog-mcp-server/src/tools/bootstrap.rs`  
**Vấn đề:** Developer mới phải tự map L2 domains. Không có starting point.

**Mở rộng `fog_bootstrap`** — thêm `--suggest-domains` mode:

```rust
// Folder-based heuristic — works for 80% projects
let suggested_domains: Vec<(String, Vec<String>)> = db.conn()
    .prepare("
        SELECT
            CASE
                WHEN f.path LIKE '%/src/%'
                THEN SUBSTR(f.path, INSTR(f.path, '/src/') + 5,
                     CASE WHEN INSTR(SUBSTR(f.path, INSTR(f.path, '/src/') + 5), '/') > 0
                          THEN INSTR(SUBSTR(f.path, INSTR(f.path, '/src/') + 5), '/') - 1
                          ELSE LENGTH(SUBSTR(f.path, INSTR(f.path, '/src/') + 5))
                     END)
                ELSE SUBSTR(f.path, 1, INSTR(f.path, '/') - 1)
            END as module,
            GROUP_CONCAT(s.name) as symbols
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        GROUP BY module
        HAVING COUNT(*) > 3
        ORDER BY COUNT(*) DESC
        LIMIT 20
    ")?
    // parse results...
```

**Output:** `fog_bootstrap` sinh thêm section trong output:
```
💡 Suggested domains (from folder structure):
   authentication (12 symbols): verify_token, login, logout...
   payment (8 symbols): process_payment, refund, checkout...
Run fog_assign to confirm each suggestion.
```

---

## 10. Feature: L4 Guardrails — Decision Coverage Warning

**File:** `crates/fog-mcp-server/src/tools/brief.rs`, `crates/fog-memory/src/query.rs`  
**Vấn đề:** L4 score binary (có/không decisions). Agent gọi `fog_decisions` 1 lần cho 1 function → score = 75/75, nhưng 95% functions vẫn undocumented.

**Thêm `decision_coverage()` vào `query.rs`:**

```rust
pub fn decision_coverage(&self) -> MemoryResult<(i64, i64)> {
    let conn = self.conn();
    // Symbols có centrality cao (thường xuyên được gọi = important)
    let high_centrality_total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM symbols WHERE centrality > 0.5",
        [], |r| r.get(0),
    ).unwrap_or(0);

    // Trong đó có bao nhiêu có decision
    let covered: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT s.name) FROM symbols s
         JOIN decisions d ON d.functions LIKE '%\"' || s.name || '\"%'
         WHERE s.centrality > 0.5",
        [], |r| r.get(0),
    ).unwrap_or(0);

    Ok((covered, high_centrality_total))
}
```

**Hiển thị trong `fog_brief`:**

```
## 📊 L4 Decision Coverage
- High-centrality functions covered: 3/12 (25%)
- Undocumented: process_payment, validate_cart, apply_discount...
- Run fog_decisions after each significant change.
```

---

## 11. Feature: Git History L4 Seed

**File:** `crates/fog-mcp-server/src/tools/bootstrap.rs`  
**Vấn đề:** L4 decisions bắt đầu trống. Git history chứa context quý giá.

**Thêm `fog_bootstrap --seed-from-git` mode:**

```rust
// Parse git log trong project_root
let output = std::process::Command::new("git")
    .args(["log", "--oneline", "-50", "--format=%H|%s|%ae"])
    .current_dir(project_root)
    .output();

if let Ok(out) = output {
    let log = String::from_utf8_lossy(&out.stdout);
    for line in log.lines() {
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() == 3 {
            let (hash, subject, _author) = (parts[0], parts[1], parts[2]);
            // Tìm function names trong commit message (heuristic: camelCase, snake_case words)
            // Tạo DRAFT decision entry với status = 'draft'
        }
    }
}
```

**Output:** Decision entries với `status = 'draft'` — agent review và confirm/reject.

---

## 12. Tool: fix fog_gaps `find_cycles` — chỉ detect self-recursion

**File:** `crates/fog-mcp-server/src/tools/gaps.rs`  
**Vấn đề:** Query hiện tại chỉ tìm `e.source_id = e.target_id` — tức là hàm gọi chính nó. Circular dependency chains thực tế (A→B→C→A) hoàn toàn không được phát hiện. Đây là bug nghiêm trọng vì tên tool cam kết "find circular dependencies".

**Thay đổi:**

```rust
// Thay thế query find_cycles bằng WITH RECURSIVE CTE
"find_cycles" => {
    // Detect true cycles using recursive CTE
    let mut stmt = conn.prepare(
        "WITH RECURSIVE cycle_search(start_id, current_id, path, depth) AS (
            SELECT source_id, target_id, CAST(source_id AS TEXT), 1
            FROM edges WHERE kind = 'CALLS' AND source_id != target_id
          UNION ALL
            SELECT cs.start_id, e.target_id,
                   cs.path || ',' || CAST(e.target_id AS TEXT),
                   cs.depth + 1
            FROM edges e
            JOIN cycle_search cs ON e.source_id = cs.current_id
            WHERE cs.depth < 6
              AND INSTR(cs.path, CAST(e.target_id AS TEXT)) = 0
        )
        SELECT DISTINCT s1.name, s2.name, cs.path
        FROM cycle_search cs
        JOIN symbols s1 ON s1.id = cs.start_id
        JOIN symbols s2 ON s2.id = cs.current_id
        WHERE cs.current_id = cs.start_id
        LIMIT 20"
    )?;
    // ...
}
```

**Đồng thời:** Rename template value `find_cycles` → keep name nhưng cập nhật description cho rõ depth limit.

---

## 13. Tool: fog_overlay auto-check trước khi chạy

**File:** `crates/fog-mcp-server/src/tools/overlay.rs`  
**Vấn đề:** `fog_overlay` fail silently nếu `security.toml` không tồn tại (trả về 0 tags applied). Agent không biết phải chạy `fog_bootstrap` trước.

**Thay đổi:**

```diff
# overlay.rs — handle()
  if !security_path.exists() && !labels_path.exists() {
-     return ToolCallResult::ok("No overlay config found. 0 tags applied.");
+     return ToolCallResult::ok(
+         "⚠️ No overlay config found.\n\
+          Run fog_bootstrap first to generate .fog-context/security.toml and labels.toml,\n\
+          then edit them for your project, then run fog_overlay again."
+     );
  }
```

---

## 14. Tool: fog_gaps `find_orphans` → rename thành `find_dead_code`

**File:** `crates/fog-mcp-server/src/tools/gaps.rs`  
**Vấn đề:** Sau Fix 3, `fog_brief` hiện "orphaned domain mappings". "Orphan" trong `fog_gaps` có nghĩa khác (dead code — symbols không có caller và không gọi ai). Hai định nghĩa "orphan" tồn tại song song → agent nhầm.

**Thay đổi:**

```diff
# gaps.rs — enum value + description
  "template": {
      "enum": [
-         "find_cycles", "find_orphans", "find_shared_callers", "find_path"
+         "find_cycles", "find_dead_code", "find_shared_callers", "find_path"
      ],
-     "description": "... dead code (find_orphans) ..."
+     "description": "... dead code/unreachable symbols (find_dead_code) ..."
  }

# Internal match arm
- "find_orphans" => { ... }
+ "find_dead_code" => { ... }
# Giữ backward compat:
+ "find_orphans" => { /* alias → find_dead_code */ }
```

---

## 15. Tool: fog_bootstrap → fog_init (rename + redescription)

**File:** `crates/fog-mcp-server/src/tools/bootstrap.rs`  
**Vấn đề:** Hiện `fog_bootstrap` chỉ tạo `security.toml` + `labels.toml`. Sau Feature 9 (suggest-domains) và Feature 11 (seed-from-git), nó trở thành universal project initializer. Tên `fog_bootstrap` không phản ánh scope mới.

**Thay đổi:**

```diff
- name: "fog_bootstrap",
- description: "Generate default security.toml and labels.toml configurations. Updates AGENTS.md.",
+ name: "fog_bootstrap",   // Giữ tên cũ để không break agents đang dùng
+ description: "Initialize fog-context for a new project. Generates security.toml, labels.toml, \
+               suggests L2 domains from folder structure, and optionally seeds L4 decisions from git history.",
  input_schema: {
      "properties": {
          "project": ...,
+         "suggest_domains": { "type": "boolean", "description": "Auto-suggest L2 domains from folder structure." },
+         "seed_git": { "type": "boolean", "description": "Seed L4 decisions from recent git commits (draft mode)." }
      }
  }
```

**Ghi chú:** Không rename tool để backward compatible với agents hiện có. Chỉ update description và thêm params.

---

## Phản hồi 4 điểm phân tích từ external reviewer

### Điểm 1 — "fog_lookup và fog_search là Direct Duplication"

**Phản hồi: Không đồng ý hoàn toàn, nhưng đúng về documentation.**

Source code cho thấy hai tool này hoạt động ở **hai tầng hoàn toàn khác nhau**:

| | fog_lookup | fog_search |
|--|-----------|-----------|
| Tìm ở đâu | SQLite AST index (symbol names, signatures) | File system (raw text content) |
| Khi dùng | Biết tên hàm/class → tìm nhanh | Tìm string literal, regex trong code |
| Index required | Có (fog_scan phải chạy trước) | Không (grep trực tiếp) |

Comment `// Replaces: search` trong `lookup.rs:2` gây nhầm — nó có nghĩa là thay thế tool `search` cũ trong TypeScript version, không phải `fog_search`. `fog_search` là capability hoàn toàn mới.

**Fix cần thiết:** Xóa comment misleading, cập nhật description `fog_search` để rõ "Use when fog_lookup fails — searches file content, not symbol index."

---

### Điểm 2 — "fog_domains và fog_assign là Action Fragmentation"

**Phản hồi: Không đồng ý — tách read/write là intentional và đúng.**

Read-write separation là design tốt cho AI agent vì:
- Agent gọi `fog_domains` thường xuyên để **orient** (không side effects)
- Agent gọi `fog_assign` để **write intentionally** (side effects)
- Merge thành 1 tool với `action` param → agent có thể accidentally trigger write khi chỉ muốn read

Tuy nhiên reviewer đúng về **token cost**. Giải pháp không phải merge mà là: `fog_domains` hiển thị hint khi domain trống — *"Use fog_assign to populate this domain"*. Giảm round-trips mà không tăng rủi ro accidental writes.

---

### Điểm 3 — "fog_outline và fog_inspect là Granularity Overlap"

**Phản hồi: Không đồng ý — khác dimension, không phải khác depth.**

- `fog_outline(path="src/auth/")` → "Cho tôi thấy tất cả symbols trong folder này" (cần path, không cần biết tên)
- `fog_inspect(name="verify_token")` → "Cho tôi 360° về symbol này" (cần biết tên)

Không thể dùng `fog_inspect` khi chưa biết tên — đó là lý do `fog_outline` tồn tại. Đây là **discovery pattern khác nhau** (location-based vs identity-based), không phải depth khác nhau.

Depth parameter như reviewer đề xuất sẽ biến `fog_outline` thành `fog_inspect` và ngược lại — confusing hơn, không ít hơn.

---

### Điểm 4 — "fog_decisions và fog_constraints là Conceptual Overlap"

**Phản hồi: Đây là điểm valid nhất. Boundary mờ trong description, không phải trong implementation.**

Source code cho thấy ranh giới **rõ ràng trong code** nhưng **mờ trong description**:

| | fog_decisions | fog_constraints |
|--|-------------|----------------|
| Loại data | Event — "Tôi đã thay đổi X vì Y" | Rule — "X không được làm Y" |
| Thời gian | Temporal (có `created_at`) | Atemporal (rule luôn đúng) |
| Layer | L4 Causality | L3 Policy |
| Đọc ADR không? | Không | Có (scan ADR files) |

**Vấn đề thật:** Description `fog_constraints` nói "Scan ADR files" — nhưng ADR (Architecture Decision Record) thường chứa CẢ decision lẫn constraint. Agent có thể dùng `fog_constraints` để nạp ADR khi thực ra nên dùng `fog_decisions`.

**Fix:** Làm rõ trong description:
- `fog_constraints` → "Rules that ALWAYS apply (architectural invariants, policies)"  
- `fog_decisions` → "Record WHY a specific change was made (temporal, per-function)"

---

## Checklist thực thi

### Fixes (không cần plan, làm ngay)
- `[ ]` **Fix 1:** Xóa L5 khỏi `knowledge_score()` — `query.rs:844`
- `[ ]` **Fix 2:** Đổi doc comments scratchpad "Layer 5" → "Agent Runtime" — `write.rs:6-7`
- `[ ]` **Fix 3:** Thêm orphan mapping query + warning vào `fog_brief` — `brief.rs`
- `[ ]` **Fix 4:** Sửa hint message tool names cũ — `query.rs:849`
- `[ ]` **Fix 5:** Thêm L2 domain coverage warning vào `fog_brief` — `brief.rs`
- `[ ]` **Fix 6:** Cập nhật layer model doc "5 layers" → "4 layers + Agent Runtime" — `lib.rs`
- `[ ]` **Fix 12:** `fog_gaps find_cycles` — SQL fix dùng WITH RECURSIVE CTE — `gaps.rs`
- `[ ]` **Fix 13:** `fog_overlay` — auto-suggest `fog_bootstrap` nếu config files không tồn tại — `overlay.rs`
- `[ ]` **Fix 14:** `fog_gaps find_orphans` → add alias `find_dead_code`, update enum — `gaps.rs`
- `[ ]` **Fix 15a:** `fog_search` description — rõ "searches file content, not symbol index" — `search.rs`
- `[ ]` **Fix 15b:** Xóa comment `// Replaces: search` misleading trong `lookup.rs:2`
- `[ ]` **Fix 15c:** `fog_constraints` + `fog_decisions` — update descriptions rõ ranh giới L3 vs L4 — `constraints.rs`, `decisions.rs`
- `[ ]` **Fix 15d:** `fog_domains` — thêm hint "Use fog_assign to populate" khi domain trống — `domains.rs`

### Features (cần phase plan trước khi code)
- `[ ]` **Feature 7:** L3 DSL — `forbidden_edge` + `tag_required` checker engine
- `[ ]` **Feature 8:** L2 domain_dependencies table + auto-calculation sau fog_scan
- `[ ]` **Feature 9:** fog_bootstrap suggest-domains mode (folder heuristic) + description update
- `[ ]` **Feature 10:** L4 decision coverage metric trong fog_brief
- `[ ]` **Feature 11:** fog_bootstrap seed-from-git mode

### Gates
- `[ ]` Build: `cargo build --package fog-memory --package fog-mcp-server` — 0 errors, 0 warnings
- `[ ]` Test: `cargo test --package fog-memory` — all pass
- `[ ]` Behavioral: `fog_brief` hiện score /75, orphan warning, coverage %
- `[ ]` Version bump: `Cargo.toml` → `0.9.1`

---

*fog-context v0.9.1 — Tổng hợp từ architectural analysis + tool overlap review*

