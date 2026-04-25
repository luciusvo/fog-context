//! fog-mcp-server/src/indexer/hints.rs
//!
//! Approach 2: Per-language hint files (.fog-context/hints/{lang}.json)
//!
//! Provides project-specific semantic bridges that can't be detected via AST:
//!   - C macro expansions (LLAMA_API → llama_api_impl)
//!   - Custom DI framework annotations
//!   - Project-specific interface → impl bindings
//!
//! AI agents: use fog_constraints({ "code": "HINT_...", "statement": "..." })
//! to add hints during a session, OR edit .fog-context/hints/{lang}.json directly.
//! Hints persist across scans as long as the JSON file exists.
//!
//! Format: .fog-context/hints/{lang}.json
//! ```json
//! {
//!   "macro_expansions": [
//!     { "pattern": "LLAMA_FUNC",  "resolves_to": "llama_func_impl" },
//!     { "pattern": "MY_HANDLER",  "resolves_to": "MyHandlerImpl::handle" }
//!   ],
//!   "di_annotations": ["@Inject", "@MyCustomAutowired"],
//!   "extra_calls": [
//!     { "from": "bootstrap",  "to": "AppKernel" }
//!   ]
//! }
//! ```
//!
//! PATTERN_DECISION: Level 1 (Pure function: path → LangHints)
//! No side effects. Errors are silently ignored (hints are optional).

use std::path::Path;
use serde_json::Value;

/// Collected hint data for a single language, loaded from hints/{lang}.json.
#[derive(Debug, Default, Clone)]
pub struct LangHints {
    /// Macro/alias expansions: (pattern_name, resolves_to_name)
    /// Creates MACRO_EXPAND edges during Pass 1.
    pub macro_expansions: Vec<(String, String)>,
    /// Additional DI annotation names to detect (supplements bridge_query).
    /// e.g. ["@MyInject", "@Autowire"]
    pub di_annotations: Vec<String>,
    /// Explicit extra call edges: (from_symbol, to_symbol)
    /// For patterns that can't be detected by any query.
    pub extra_calls: Vec<(String, String)>,
}

impl LangHints {
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.macro_expansions.is_empty()
            && self.di_annotations.is_empty()
            && self.extra_calls.is_empty()
    }
}

/// Load hints for a specific language from .fog-context/hints/{lang}.json.
/// Returns empty LangHints (not an error) if file doesn't exist or is malformed.
/// Errors are intentionally swallowed — hints are advisory, not required.
pub fn load(project_root: &Path, lang: &str) -> LangHints {
    let path = project_root.join(".fog-context").join("hints").join(format!("{lang}.json"));
    if !path.exists() {
        return LangHints::default();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return LangHints::default(),
    };
    let v: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return LangHints::default(),
    };

    let mut hints = LangHints::default();

    // macro_expansions: [{ "pattern": "...", "resolves_to": "..." }]
    if let Some(arr) = v["macro_expansions"].as_array() {
        for item in arr {
            if let (Some(p), Some(r)) = (item["pattern"].as_str(), item["resolves_to"].as_str()) {
                hints.macro_expansions.push((p.to_string(), r.to_string()));
            }
        }
    }

    // di_annotations: ["@MyInject", ...]
    if let Some(arr) = v["di_annotations"].as_array() {
        for item in arr {
            if let Some(s) = item.as_str() {
                hints.di_annotations.push(s.to_string());
            }
        }
    }

    // extra_calls: [{ "from": "...", "to": "..." }]
    if let Some(arr) = v["extra_calls"].as_array() {
        for item in arr {
            if let (Some(f), Some(t)) = (item["from"].as_str(), item["to"].as_str()) {
                hints.extra_calls.push((f.to_string(), t.to_string()));
            }
        }
    }

    hints
}

/// Write .fog-context/hints/_template.json on first fog_scan if hints/ dir missing.
/// Idempotent — does nothing if the dir or any hint file already exists.
pub fn write_template_if_missing(project_root: &Path) {
    let hints_dir = project_root.join(".fog-context").join("hints");
    if hints_dir.exists() {
        return; // Already initialized
    }
    if std::fs::create_dir_all(&hints_dir).is_err() {
        return;
    }
    let template = hints_dir.join("_template.json");
    let content = r#"{
  "_comment": "fog-context language hint file. Copy and rename to {lang}.json (e.g. java.json, c.json).",
  "_docs": "https://github.com/luciusvo/fog-context#hint-files",

  "macro_expansions": [
    { "_example": true, "pattern": "MY_MACRO_NAME", "resolves_to": "actual_function_name" }
  ],

  "di_annotations": [
    "_example: @MyCustomInject"
  ],

  "extra_calls": [
    { "_example": true, "from": "bootstrap_function", "to": "AppKernel" }
  ]
}
"#;
    let _ = std::fs::write(template, content);
}
