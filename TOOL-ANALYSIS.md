# Tool Inventory Analysis — fog-context v0.9.1

## Tổng quan: 17 tools hiện tại

| # | Tool | Group | Chức năng core |
|---|------|-------|----------------|
| 1 | fog_roots | Core | List all registered projects |
| 2 | fog_scan | Core | Index/re-index codebase (L1) |
| 3 | fog_lookup | Core | BM25 symbol search |
| 4 | fog_outline | Core | File/dir symbol list (token-efficient) |
| 5 | fog_inspect | Core | 360° symbol context |
| 6 | fog_impact | Core | Blast radius analysis |
| 7 | fog_trace | Core | Call tree traversal |
| 8 | fog_brief | Core | Project health + knowledge score |
| 9 | fog_gaps | Advanced | Graph analysis (cycles, orphans, path) |
| 10 | fog_domains | Advanced | L2 domain catalog + query |
| 11 | fog_assign | Advanced | L2 write — link symbols to domain |
| 12 | fog_constraints | Advanced | L3 write — ingest ADRs + inline inject |
| 13 | fog_decisions | Advanced | L4 write — record WHY |
| 14 | fog_import | Advanced | Import L2/L3/L4 from ByteRover/GitNexus |
| 15 | fog_search | Advanced | Raw text/regex file search |
| 16 | fog_overlay | Security | Apply security.toml/labels.toml → symbol_tags |
| 17 | fog_bootstrap | Security | Generate security.toml + labels.toml |

---

## Phân tích chồng chéo

### 🔴 CHỒNG CHÉO CAO — fog_lookup vs fog_outline

**Tình trạng:** Cả hai đều "tìm symbols", agent thường thử cả hai khi không tìm thấy.

| | fog_lookup | fog_outline |
|--|-----------|------------|
| Input | query string (tên symbol) | path (file/dir) |
| Cách tìm | BM25 full-text trên symbol names | Filter by file path prefix |
| Output | List symbols match query, ranked | All symbols trong file/dir đó |
| Use case | "Tìm hàm có tên gần với X" | "Cho tôi xem tất cả hàm trong file Y" |

**Verdict:** Không phải chồng chéo thật — dimension khác nhau (name-based vs path-based). Giữ cả hai, nhưng **mô tả cần rõ hơn** để agent không thử cả hai cho cùng task.

---

### 🔴 CHỒNG CHÉO CAO — fog_overlay vs fog_bootstrap

**Tình trạng:** Agent hay gọi nhầm hoặc gọi sai thứ tự. Description không rõ dependency.

| | fog_bootstrap | fog_overlay |
|--|--------------|------------|
| Làm gì | Tạo file `security.toml` + `labels.toml` | Đọc file đó → ghi vào `symbol_tags` DB |
| Khi nào | Lần đầu setup | Sau khi sửa toml files |
| Input | project | project |
| Output | Files trên disk | DB rows |

**Thật ra** đây là 2 bước bắt buộc theo thứ tự:
```
fog_bootstrap → (edit .fog-context/security.toml) → fog_overlay
```

**Vấn đề:** Không có cơ chế enforce thứ tự. `fog_overlay` sẽ fail silently nếu chạy trước `fog_bootstrap` (files không tồn tại).

**Giải pháp:** `fog_overlay` auto-check xem files có tồn tại không, nếu không → suggest chạy `fog_bootstrap` trước. Hoặc merge cả hai thành `fog_overlay` với `--init` flag.

---

### 🟡 CHỒNG CHÉO TRUNG BÌNH — fog_assign vs fog_import (L2 write path)

| | fog_assign | fog_import |
|--|-----------|-----------|
| Làm gì | Ghi L2 domain + link symbols | Import L2/L3/L4 từ file formats khác |
| Source | Agent input | .brv/ hoặc .gitnexus/ files |
| Scope | 1 domain tại 1 thời điểm | Batch import nhiều domains |

**Verdict:** Không thật sự chồng chéo — `fog_import` là migration tool (onetime), `fog_assign` là everyday write tool. Tuy nhiên sau khi có **Feature 9 (bootstrap suggest-domains)**, `fog_bootstrap` cũng sẽ ghi L2 data → tạo thêm 1 write path cho L2.

**Tiềm năng xung đột sau v0.9.1:** fog_bootstrap (suggest-domains) + fog_assign + fog_import đều ghi vào `domain_symbols`. Cần đảm bảo tất cả dùng UPSERT (`ON CONFLICT DO NOTHING`) — hiện `fog_assign` đúng (`INSERT OR IGNORE`).

---

### 🟡 CHỒNG CHÉO TRUNG BÌNH — fog_inspect vs fog_impact

| | fog_inspect | fog_impact |
|--|------------|-----------|
| Input | symbol name | symbol name |
| Focus | Context của 1 symbol (callers, callees, tags, decisions) | Blast radius nếu thay đổi symbol này |
| Output | Markdown 360° view | Risk level + caller tree |
| Staleness check | Chỉ khi có file hint | Luôn check |

**Overlap:** Cả hai đều list callers. `fog_inspect` list 1 cấp callers, `fog_impact` traversal 3 cấp (depth configurable).

**Verdict:** Use case khác nhau rõ ràng: `fog_inspect` = "tôi muốn hiểu symbol này", `fog_impact` = "tôi muốn biết rủi ro nếu sửa symbol này". Nhưng nhiều agent gọi `fog_inspect` trước rồi `fog_impact` cho cùng 1 symbol — 2 round trips, dữ liệu overlap.

**Cải thiện:** `fog_inspect` nên thêm 1 dòng hint khi callers > threshold: *"High centrality symbol — run fog_impact before modifying."* Không merge tool, chỉ cross-link.

---

### 🟢 ÍT HỮU ÍCH HƠN SAU v0.9.1 — fog_gaps (find_cycles)

**Vấn đề:** `find_cycles` hiện chỉ detect **self-recursion** (`WHERE e.source_id = e.target_id`) — không detect circular dependency chains (A→B→C→A). Đây là limitation lớn vì circular deps trong thực tế hầu như không phải self-call.

```sql
-- Hiện tại (gaps.rs:104)
WHERE e.source_id = e.target_id AND e.kind = 'CALLS'
-- Chỉ tìm: fn foo() { foo(); } -- self call
-- BỎ SÓT: fn a() { b(); } fn b() { c(); } fn c() { a(); }
```

**Impact sau Feature 8 (L2 domain_dependencies):** Khi có `domain_dependencies` table, circular domain dependency sẽ dễ detect hơn nhiều chỉ bằng 1 query đơn giản. `fog_gaps find_cycles` trở nên deprecated hơn cho use case này.

**Recommendation:** Fix `find_cycles` để dùng `WITH RECURSIVE` CTE thật sự, hoặc đổi tên thành `find_self_recursion` cho accurate.

---

### 🟢 ÍT HỮU ÍCH HƠN SAU v0.9.1 — fog_gaps (find_orphans) sau Feature 3

**Tình trạng hiện tại:** `find_orphans` tìm symbols không có edges nào (cả gọi lẫn bị gọi).

**Sau Fix 3 (orphan domain mappings):** `fog_brief` sẽ tự động hiện orphaned **domain mappings**. Hai loại orphan khác nhau:
- `fog_gaps find_orphans` → symbols không có callers/callees (dead code)
- `fog_brief orphan check` → symbols trong domains mà không còn trong code

Không thật sự conflict nhưng agent có thể nhầm lẫn "orphan" nghĩa là gì.

**Recommendation:** Đổi tên `find_orphans` thành `find_dead_code` cho rõ nghĩa.

---

### 🟢 ÍT HỮU ÍCH SAU v0.9.1 — fog_bootstrap scope quá hẹp

**Hiện tại:** `fog_bootstrap` chỉ tạo `security.toml` + `labels.toml` + cập nhật AGENTS.md.

**Sau Feature 9:** `fog_bootstrap` sẽ thêm `--suggest-domains` mode.  
**Sau Feature 11:** `fog_bootstrap` sẽ thêm `--seed-from-git` mode.

**Vấn đề:** Description hiện tại `"Generate default security.toml and labels.toml configurations"` sẽ sai hoàn toàn. Tool này đang tiến hóa thành **universal project initializer** — một "wizard" setup toàn bộ fog-context cho project mới.

**Recommendation:** Rename + redescription thành `fog_init` hoặc ít nhất update description trước mỗi feature bổ sung.

---

## Tóm tắt

| Tool | Verdict | Action |
|------|---------|--------|
| fog_lookup vs fog_outline | Không chồng chéo, dimension khác | Cải thiện description |
| fog_overlay vs fog_bootstrap | Chồng chéo thứ tự | `fog_overlay` auto-suggest bootstrap nếu files không tồn tại |
| fog_assign vs fog_import vs bootstrap | Cùng write L2, khác source | Đảm bảo tất cả UPSERT safe |
| fog_inspect vs fog_impact | Overlap callers section | `fog_inspect` add centrality hint → fog_impact |
| fog_gaps find_cycles | Bug: chỉ detect self-recursion | Fix SQL dùng WITH RECURSIVE CTE |
| fog_gaps find_orphans | Naming ambiguity sau Fix 3 | Rename → `find_dead_code` |
| fog_bootstrap | Scope sẽ mở rộng v0.9.1 | Plan rename → fog_init hoặc cập nhật description |
