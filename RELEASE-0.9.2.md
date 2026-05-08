# fog-context v0.9.2 — Bug Fix & Layer Enhancement Release

> Scope: Correctness fixes phát hiện từ code audit (arch-review-layers-bugs.md).  
> Không thay đổi kiến trúc lớn. Tiếp nối v0.9.1.

---

## Nguyên tắc

- **Sửa bug trước, feature sau** — 0.9.2 là pure bug fix + layer data model improvements
- Mỗi fix có exact file + line reference từ codebase hiện tại
- BUG-002 → BUG-005 đã có trong 0.9.1 checklist — không lặp lại ở đây

---

## BUG-006 — decisions LIKE query không escape wildcard characters

**File:** `crates/fog-memory/src/query.rs:313-325` và `:451-463`  
**Severity:** 🟡 MEDIUM  
**Triệu chứng:** `fog_inspect("symbol_with_%_in_name")` trả về decisions sai hoặc thiếu.  
Nếu symbol name chứa `%`, `_`, hoặc `"` → SQLite LIKE wildcard → false matches.

**Root cause:**
```rust
// query.rs:318 — hiện tại
rusqlite::params![format!("%\"{name}\"%")]
// Nếu name = "format_user_data" → pattern = %"format_user_data"%  ← đúng
// Nếu name = "auth%bypass"     → pattern = %"auth%bypass"% ← SQL wildcard bug!
// Nếu name = "foo\"bar"         → pattern bị break JSON quoting
```

**Fix: Dùng json_each() thay LIKE** — exact match, zero escape issues:

```diff
# query.rs:313-325 (context_symbol - first occurrence)
- let mut decisions_stmt = conn.prepare(
-     "SELECT id, reason, revert_risk, status, created_at FROM decisions
-      WHERE functions LIKE ?1 ORDER BY created_at DESC LIMIT 10",
- ).map_err(crate::MemoryError::Database)?;
- let decisions = decisions_stmt.query_map(
-     rusqlite::params![format!("%\"{name}\"\"%")],

+ let mut decisions_stmt = conn.prepare(
+     "SELECT id, reason, revert_risk, status, created_at FROM decisions
+      WHERE EXISTS (
+          SELECT 1 FROM json_each(functions) WHERE value = ?1
+      )
+      ORDER BY created_at DESC LIMIT 10",
+ ).map_err(crate::MemoryError::Database)?;
+ let decisions = decisions_stmt.query_map(
+     rusqlite::params![name],
```

Áp dụng tương tự cho `:451-463` (domain_catalog query, second occurrence).

**Lý do chọn json_each():** SQLite 3.38+ (2022) hỗ trợ json_each() là built-in. Exact value match — không cần escape, không có wildcard ambiguity. Performance tương đương LIKE trên datasets nhỏ (<10k decisions).

---

## BUG-007 — fog_outline không chặn path traversal (`..`)

**File:** `crates/fog-mcp-server/src/tools/outline.rs:37`  
**Severity:** 🟡 MEDIUM  
**Triệu chứng:** `fog_outline(path="../../../etc")` → skeleton query chạy với path ngoài project.

**Root cause:**
```rust
// outline.rs:37 — hiện tại
let is_root = matches!(path, "." | "./" | "/" | "");
// Bỏ sót: "..", "../", absolute paths, symlink traversal
```

**Fix: Canonicalize và sandbox check trước khi query:**

```diff
# outline.rs:31-51 — replace path validation block
  let path = match args["path"].as_str() {
      Some(p) if !p.is_empty() => p,
      _ => return ToolCallResult::err("fog_outline: 'path' is required"),
  };

- let is_root = matches!(path, "." | "./" | "/" | "");
- if is_root {
-     return ToolCallResult::ok("⚠️  fog_outline does not support root directory...");
- }

+ // Sandbox: resolve path relative to project_root, reject traversal
+ let joined = project_root.join(path);
+ let canonical = joined.canonicalize().unwrap_or(joined.clone());
+ let project_canonical = project_root.canonicalize()
+     .unwrap_or_else(|_| project_root.to_path_buf());
+
+ if !canonical.starts_with(&project_canonical) {
+     return ToolCallResult::err(format!(
+         "fog_outline: path '{}' is outside project root. Use relative paths only.",
+         path
+     ));
+ }
+
+ // Root path = too many results
+ if canonical == project_canonical {
+     return ToolCallResult::ok(
+         "⚠️  fog_outline does not support root directory - it would return too many symbols.\n\
+          Specify a file or subdirectory instead:\n\
+          ```\n\
+          fog_outline({ \"path\": \"src/\" })          ← outline a directory\n\
+          fog_outline({ \"path\": \"src/main.rs\" })   ← outline a single file\n\
+          ```\n\
+          To search symbols across the whole codebase, use:\n\
+          ```\n\
+          fog_lookup({ \"query\": \"your_function_name\" })\n\
+          ```"
+     );
+ }
+
+ // Use relative path for DB query (skeleton uses path prefix matching)
+ let rel_path = canonical.strip_prefix(&project_canonical)
+     .map(|p| p.to_string_lossy().replace('\\', "/"))
+     .unwrap_or_else(|_| path.to_string());
+ let path = rel_path.as_str();
```

---

## BUG-008 — fog_search sandbox check fail với symlinks

**File:** `crates/fog-mcp-server/src/tools/search.rs:71-79`  
**Severity:** 🟡 MEDIUM  
**Triệu chứng:** `c.starts_with(project_root)` có thể fail false negative nếu `project_root` chứa symlink components hay trailing slash.

**Root cause:**
```rust
// search.rs:73 — hiện tại
if !c.starts_with(project_root) {
//              ^^^^^^^^^^^^^^^^^^^
// c = canonicalized (symlinks resolved, absolute)
// project_root = NOT canonicalized (may contain symlinks)
// → starts_with so sánh byte-by-byte → mismatch nếu project_root có symlink
```

**Fix: Canonicalize cả project_root:**

```diff
# search.rs:65-82 — run_search() path validation
  fn run_search(args: SearchArgs, project_root: &Path) -> ToolCallResult {
+     // Pre-canonicalize project_root once for consistent sandbox comparison
+     let canonical_root = project_root.canonicalize()
+         .unwrap_or_else(|_| project_root.to_path_buf());
+
      let search_root = if let Some(p) = &args.path {
          let p_path = Path::new(p);
          let joined = project_root.join(p_path);
          match joined.canonicalize() {
              Ok(c) => {
-                 if !c.starts_with(project_root) {
+                 if !c.starts_with(&canonical_root) {
                      return ToolCallResult::err("Path must be inside project root".to_string());
                  }
                  c
              }
              Err(_) => return ToolCallResult::err(format!("Path does not exist: {}", p)),
          }
      } else {
-         project_root.to_path_buf()
+         canonical_root.clone()
      };
```

---

## BUG-009 — HINT_ constraint prefix tự động uppercase nhưng không document rõ

**File:** `crates/fog-mcp-server/src/tools/constraints.rs:250`  
**Severity:** 🟢 LOW — Không phải bug logic, chỉ là UX confusion  
**Triệu chứng:** Agent viết `hint_my_edge` → lưu thành `HINT_MY_EDGE`. Agent không biết code thật là gì khi query lại.

**Fix: Thêm note vào output message:**

```diff
# constraints.rs:297-303 — handle_raw_text_file output
  ToolCallResult::ok(format!(
      "✅ **Raw text ingested:** {}\n\
-      - **Imported:** {imported} constraints into Layer 3\n\
-      - **Tip:** Use `HINT_NAME: description` lines to record semantic bridges\
+      - **Imported:** {imported} constraints into Layer 3\n\
+      - **Note:** All constraint codes are stored UPPERCASE (e.g., `hint_foo` → `HINT_FOO`)\n\
+      - **Tip:** Use `HINT_NAME: description` lines to record semantic bridges\
       {warn}",
      path.display()
  ))
```

---

## BUG-010 — `validated` flag partial match misleading

**File:** `crates/fog-memory/src/write.rs:78-88`  
**Severity:** 🟢 LOW  
**Triệu chứng:** `fog_decisions(functions=["process_payment", "nonexistent"])` → `validated = true` dù `nonexistent` không trong index.

**Root cause:**
```rust
// write.rs:87
count > 0  // = "ít nhất 1 tồn tại" → true kể cả khi chỉ 1/N match
```

**Fix: Phân biệt `fully_validated` vs `partially_validated`:**

```diff
# write.rs:78-100
  let validated = if args.functions.is_empty() {
      false
  } else {
      let sql = format!("SELECT COUNT(*) FROM symbols WHERE name IN ({placeholders})");
      let mut stmt = conn.prepare(&sql)?;
      let count: i64 = stmt.query_row(
          rusqlite::params_from_iter(args.functions.iter()), |row| row.get(0)
      ).unwrap_or(0);
-     count > 0
+     count == args.functions.len() as i64  // Only true if ALL functions exist
  };

+ // Warn if partial match — functions not in index (may be new code not yet scanned)
+ let unresolved_count = args.functions.len() as i64 - count;
+ if unresolved_count > 0 {
+     tracing::warn!(
+         "fog-memory: {unresolved_count}/{} functions not found in symbol index — \
+          run fog_scan to update index before recording decisions",
+         args.functions.len()
+     );
+ }
```

---

## L4 Enhancement — Decision granularity (file_path + line_range)

**Files:** `crates/fog-memory/src/db.rs`, `write.rs`, `query.rs`  
**Vấn đề:** Line-level decisions bị mất trong `reason` text không query được.  
**Phát hiện từ:** arch-review-layers-bugs.md §1B

**Schema migration:**

```sql
-- db.rs — thêm vào SCHEMA_SQL (sau decisions table definition)
-- Migration: add granularity fields to decisions
-- Chạy trong ensure_schema() với idempotent ALTER TABLE pattern
ALTER TABLE decisions ADD COLUMN file_path TEXT;
-- Optional: "src/auth/token.rs"

ALTER TABLE decisions ADD COLUMN line_range TEXT;
-- Optional: "45-50"

ALTER TABLE decisions ADD COLUMN granularity TEXT NOT NULL DEFAULT 'function';
-- 'domain'   → domain-level architectural decision
-- 'function' → function-level implementation decision (default)
-- 'line'     → inline code-level annotation
```

**ensure_schema() migration pattern** (idempotent):

```rust
// db.rs — trong ensure_schema(), sau SCHEMA_SQL execution
for col in [
    ("decisions", "file_path", "TEXT"),
    ("decisions", "line_range", "TEXT"),
    ("decisions", "granularity", "TEXT NOT NULL DEFAULT 'function'"),
] {
    let _ = conn.execute_batch(&format!(
        "ALTER TABLE {} ADD COLUMN {} {}",
        col.0, col.1, col.2
    ));
    // Ignore error = column already exists (SQLite returns error on duplicate ALTER TABLE)
}
```

**Write path** (`write.rs` — `RecordDecisionArgs`):

```diff
# write.rs — RecordDecisionArgs struct
  pub struct RecordDecisionArgs {
      pub functions: Vec<String>,
      pub reason: String,
      pub domain: Option<String>,
      pub revert_risk: Option<String>,
      pub supersedes_id: Option<i64>,
+     pub file_path: Option<String>,    // e.g. "src/auth/token.rs"
+     pub line_range: Option<String>,   // e.g. "45-50"
+     pub granularity: Option<String>,  // "domain" | "function" | "line"
  }

# write.rs — INSERT SQL
- "INSERT INTO decisions (domain, functions, reason, revert_risk, validated, status)
-  VALUES (?1, ?2, ?3, ?4, ?5, 'active')",
+ "INSERT INTO decisions (domain, functions, reason, revert_risk, validated, status,
+                         file_path, line_range, granularity)
+  VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8)",
  rusqlite::params![
      args.domain,
      serde_json::to_string(&args.functions)?,
      args.reason,
      args.revert_risk.as_deref().unwrap_or("LOW"),
      validated as i64,
+     args.file_path,
+     args.line_range,
+     args.granularity.as_deref().unwrap_or("function"),
  ],
```

**Read path** (`query.rs` — `context_symbol()`):

```diff
# query.rs:313-315 — thêm file_path vào query context
# Khi biết file của symbol, match cả line-level decisions trong cùng file:

  let mut decisions_stmt = conn.prepare(
      "SELECT id, reason, revert_risk, status, created_at FROM decisions
       WHERE EXISTS (
           SELECT 1 FROM json_each(functions) WHERE value = ?1
+      ) OR (granularity = 'line' AND file_path = ?2)
       ORDER BY created_at DESC LIMIT 10",
  )?;
  let decisions = decisions_stmt.query_map(
-     rusqlite::params![name],
+     rusqlite::params![name, sym_file],
```

**Tool input schema** (`decisions.rs`):

```diff
# decisions.rs — input_schema JSON
  "properties": {
      "functions": ...,
      "reason": ...,
      "domain": ...,
      "revert_risk": ...,
      "supersedes_id": ...,
+     "file_path": {
+         "type": "string",
+         "description": "[line granularity] File where the decision applies. e.g. 'src/auth/token.rs'"
+     },
+     "line_range": {
+         "type": "string",
+         "description": "[line granularity] Line range. e.g. '45-50'"
+     },
+     "granularity": {
+         "type": "string",
+         "enum": ["domain", "function", "line"],
+         "description": "Scope of this decision. Default: 'function'."
+     }
  }
```

**Display trong fog_inspect** (`inspect.rs`):

```
## 📝 Decisions (L4)
| Granularity | Scope | Risk | Date |
|-------------|-------|------|------|
| function | validate_token — Switch to RS256 | HIGH | 2026-05-01 |
| line:45-50 | src/auth/token.rs — Use saturating_sub | LOW | 2026-05-07 |
| domain | Auth — Adopt DDD for auth module | MEDIUM | 2026-04-20 |
```

---

## L3 Enhancement — Discriminated union rule_type

**Files:** `crates/fog-memory/src/db.rs`, `crates/fog-mcp-server/src/tools/inspect.rs`  
**Từ phân tích:** Không tách layer L3a/L3b. Thêm discriminator vào cùng table.  
**Đây là Feature 7 từ RELEASE-0.9.1 — thêm implementation detail ở đây.**

**Schema migration (idempotent, same pattern as L4):**

```rust
// db.rs — ensure_schema() additions
for col in [
    ("constraints", "rule_type", "TEXT NOT NULL DEFAULT 'prose'"),
    ("constraints", "rule_config", "TEXT"),
] {
    let _ = conn.execute_batch(&format!(
        "ALTER TABLE {} ADD COLUMN {} {}", col.0, col.1, col.2
    ));
}
```

**rule_type values:**

| Value | rule_config format | Khi nào dùng |
|-------|-------------------|--------------|
| `prose` | NULL | Mọi constraint hiện tại — text guidance |
| `forbidden_edge` | `{"from_tag":"http","to_tag":"db"}` | Kiến trúc edge policy |
| `tag_required` | `{"has_tag":"pii","must_also_have":"audited"}` | Security compliance |

**Display trong fog_inspect** — thêm cột Type:

```diff
# inspect.rs — constraints section
- "| `{code}` | {sev} | {statement} |\n"
+ "| `{code}` | {rule_type_icon} | {sev} | {statement} |\n"
# rule_type_icon: prose→📝, forbidden_edge→🚫, tag_required→✅
```

---

## Checklist thực thi

### Bug fixes (Ưu tiên thực hiện theo thứ tự)
- `[ ]` **BUG-006:** Replace LIKE với json_each() trong `query.rs` (2 chỗ: :315, :453)
- `[ ]` **BUG-007:** Thêm canonicalize + sandbox check vào `outline.rs:37-51`
- `[ ]` **BUG-008:** Canonicalize `project_root` trong `search.rs:65-82`
- `[ ]` **BUG-009:** Thêm UPPERCASE note vào `constraints.rs` output message
- `[ ]` **BUG-010:** Đổi `count > 0` → `count == functions.len()` trong `write.rs:87`

### Layer enhancements (cần migration pattern)
- `[ ]` **L4-ENH:** Schema migration thêm `file_path`, `line_range`, `granularity` — `db.rs`
- `[ ]` **L4-ENH:** Update `RecordDecisionArgs` + INSERT SQL — `write.rs`
- `[ ]` **L4-ENH:** Update `context_symbol` query — `query.rs:313`
- `[ ]` **L4-ENH:** Update `fog_decisions` input schema — `decisions.rs`
- `[ ]` **L4-ENH:** Update `fog_inspect` decision display — `inspect.rs`
- `[ ]` **L3-ENH:** Schema migration `rule_type` + `rule_config` — `db.rs`
- `[ ]` **L3-ENH:** Update `fog_inspect` constraint display với type icon — `inspect.rs`

### Gates
- `[ ]` Build: `cargo build --package fog-memory --package fog-mcp-server` — 0 errors
- `[ ]` Test: `cargo test --package fog-memory` — all pass
- `[ ]` Regression: `fog_inspect("process_payment")` → decisions hiện đúng với json_each
- `[ ]` Regression: `fog_outline(path="../etc")` → trả về error, không panic
- `[ ]` Regression: `fog_search(path="../etc")` → trả về error với symlink project_root
- `[ ]` Regression: `fog_decisions(functions=["real_fn","ghost"])` → `validated=false`
- `[ ]` Migration: DB từ 0.9.1 lên 0.9.2 — cột mới có DEFAULT, không break old data
- `[ ]` Version bump: `Cargo.toml` → `0.9.2`

---

*fog-context v0.9.2 — Bug fix + Layer data model improvements*  
*Source: arch-review-layers-bugs.md | Cross-validated: query.rs, write.rs, search.rs, outline.rs, db.rs*
