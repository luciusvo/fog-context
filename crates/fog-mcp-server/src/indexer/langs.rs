//! fog-mcp-server/src/indexer/langs.rs
//!
//! Language detection and Tree-sitter parser configuration.
//! Maps file extensions → (language name, tree-sitter Language, AST queries).
//!
//! Supported (15 language configs - TypeScript and TSX are handled separately):
//!   Rust · TypeScript · TSX/JSX · Python
//!   Go · C · C++ · Java · C# · Ruby · PHP · Kotlin · Lua · Swift · Dart
//!
//! Capture name convention:
//!   @name  - the identifier node (function/class name)
//!   @def   - the full definition node (for line ranges + signature)
//!   @call  - the call site node
//!   Pattern order in def_query → cfg.kinds[pattern_index]
//!
//! PATTERN_DECISION: Level 3 (HOF + dispatch map - ext → LangConfig)

use tree_sitter::Language;
#[cfg(any(feature = "kotlin", feature = "swift", feature = "dart"))]
use tree_sitter_language::LanguageFn;

#[cfg(feature = "kotlin")]
extern "C" { fn tree_sitter_kotlin() -> *const (); }
#[cfg(feature = "swift")]
extern "C" { fn tree_sitter_swift() -> *const (); }
#[cfg(feature = "dart")]
extern "C" { fn tree_sitter_dart() -> *const (); }

#[cfg(feature = "kotlin")]
pub fn get_kotlin() -> Language {
    unsafe { LanguageFn::from_raw(tree_sitter_kotlin).into() }
}
#[cfg(feature = "swift")]
pub fn get_swift() -> Language {
    unsafe { LanguageFn::from_raw(tree_sitter_swift).into() }
}
#[cfg(feature = "dart")]
pub fn get_dart() -> Language {
    unsafe { LanguageFn::from_raw(tree_sitter_dart).into() }
}

/// Configuration for a single language's Tree-sitter parsing.
pub struct LangConfig {
    pub name: &'static str,
    pub ts_language: Language,
    pub def_query: &'static str,
    pub call_query: &'static str,
    /// Symbol kinds by pattern_index (same order as def_query patterns)
    pub kinds: &'static [&'static str],
    // ── Bridge Query (Approach 1 — built-in framework pattern detection) ──
    // Captures DI annotations, interface implementations, dynamic imports, etc.
    // Each capture @name produces a Deferred edge with `bridge_edge_kind`.
    // None = language has no well-known bridge patterns to detect.
    pub bridge_query: Option<&'static str>,
    /// edge_kind string to use for all edges produced by bridge_query.
    /// e.g. "DI_INJECT", "IMPLEMENTS", "DYNAMIC_IMPORT"
    pub bridge_edge_kind: &'static str,
}

/// Map a file extension to a canonical language name.
pub fn lang_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        // Rust
        "rs"                            => Some("rust"),
        // TypeScript (pure TS, no JSX)
        "ts" | "mts" | "cts"           => Some("typescript"),
        // TSX / JSX - uses separate grammar with JSX support
        "tsx" | "jsx" | "js" | "mjs" | "cjs" => Some("tsx"),
        // Python
        "py" | "pyi"                    => Some("python"),
        // Go
        "go"                            => Some("go"),
        // C / C++
        "c" | "h"                       => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        // Java
        "java"                          => Some("java"),
        // C#
        "cs"                            => Some("csharp"),
        // Ruby
        "rb" | "rake" | "gemspec"       => Some("ruby"),
        // PHP
        "php" | "php7" | "php8"         => Some("php"),
        // Kotlin
        #[cfg(feature = "kotlin")]
        "kt" | "kts"                    => Some("kotlin"),
        // Swift
        #[cfg(feature = "swift")]
        "swift"                         => Some("swift"),
        // Dart
        #[cfg(feature = "dart")]
        "dart"                          => Some("dart"),
        // Lua
        "lua"                           => Some("lua"),
        _                               => None,
    }
}

/// Load LangConfig for a language name. Returns None for unsupported langs.
pub fn config_for(lang: &str) -> Option<LangConfig> {
    match lang {
        "rust" => Some(LangConfig {
            name: "rust",
            ts_language: tree_sitter_rust::LANGUAGE.into(),
            def_query: RUST_DEF_QUERY,
            call_query: RUST_CALL_QUERY,
            kinds: RUST_KINDS,
            bridge_query: None, // Rust: #[derive] handled via call_query macro captures
            bridge_edge_kind: "CALLS",
        }),
        "typescript" => Some(LangConfig {
            name: "typescript",
            ts_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            def_query: TS_DEF_QUERY,
            call_query: TS_CALL_QUERY,
            kinds: TS_KINDS,
            bridge_query: Some(TS_BRIDGE_QUERY),
            bridge_edge_kind: "DYNAMIC_IMPORT",
        }),
        "tsx" => Some(LangConfig {
            name: "tsx",
            ts_language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            def_query: TSX_DEF_QUERY,
            call_query: TS_CALL_QUERY,
            kinds: TSX_KINDS,
            bridge_query: Some(TS_BRIDGE_QUERY),
            bridge_edge_kind: "DYNAMIC_IMPORT",
        }),
        "python" => Some(LangConfig {
            name: "python",
            ts_language: tree_sitter_python::LANGUAGE.into(),
            def_query: PY_DEF_QUERY,
            call_query: PY_CALL_QUERY,
            kinds: PY_KINDS,
            bridge_query: Some(PY_BRIDGE_QUERY),
            bridge_edge_kind: "DECORATES",
        }),
        "go" => Some(LangConfig {
            name: "go",
            ts_language: tree_sitter_go::LANGUAGE.into(),
            def_query: GO_DEF_QUERY,
            call_query: GO_CALL_QUERY,
            kinds: GO_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        "c" => Some(LangConfig {
            name: "c",
            ts_language: tree_sitter_c::LANGUAGE.into(),
            def_query: C_DEF_QUERY,
            call_query: C_CALL_QUERY,
            kinds: C_KINDS,
            bridge_query: None, // C macros: handled via hints/c.json
            bridge_edge_kind: "CALLS",
        }),
        "cpp" => Some(LangConfig {
            name: "cpp",
            ts_language: tree_sitter_cpp::LANGUAGE.into(),
            def_query: CPP_DEF_QUERY,
            call_query: CPP_CALL_QUERY,
            kinds: CPP_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        "java" => Some(LangConfig {
            name: "java",
            ts_language: tree_sitter_java::LANGUAGE.into(),
            def_query: JAVA_DEF_QUERY,
            call_query: JAVA_CALL_QUERY,
            kinds: JAVA_KINDS,
            bridge_query: Some(JAVA_BRIDGE_QUERY),
            bridge_edge_kind: "DI_INJECT",
        }),
        "csharp" => Some(LangConfig {
            name: "csharp",
            ts_language: tree_sitter_c_sharp::LANGUAGE.into(),
            def_query: CS_DEF_QUERY,
            call_query: CS_CALL_QUERY,
            kinds: CS_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        "ruby" => Some(LangConfig {
            name: "ruby",
            ts_language: tree_sitter_ruby::LANGUAGE.into(),
            def_query: RUBY_DEF_QUERY,
            call_query: RUBY_CALL_QUERY,
            kinds: RUBY_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        "php" => Some(LangConfig {
            name: "php",
            ts_language: tree_sitter_php::LANGUAGE_PHP.into(),
            def_query: PHP_DEF_QUERY,
            call_query: PHP_CALL_QUERY,
            kinds: PHP_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        #[cfg(feature = "kotlin")]
        "kotlin" => Some(LangConfig {
            name: "kotlin",
            ts_language: get_kotlin(),
            def_query: KOTLIN_DEF_QUERY,
            call_query: KOTLIN_CALL_QUERY,
            kinds: KOTLIN_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        #[cfg(feature = "swift")]
        "swift" => Some(LangConfig {
            name: "swift",
            ts_language: get_swift(),
            def_query: SWIFT_DEF_QUERY,
            call_query: SWIFT_CALL_QUERY,
            kinds: SWIFT_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        #[cfg(feature = "dart")]
        "dart" => Some(LangConfig {
            name: "dart",
            ts_language: get_dart(),
            def_query: DART_DEF_QUERY,
            call_query: DART_CALL_QUERY,
            kinds: DART_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        "lua" => Some(LangConfig {
            name: "lua",
            ts_language: tree_sitter_lua::LANGUAGE.into(),
            def_query: LUA_DEF_QUERY,
            call_query: LUA_CALL_QUERY,
            kinds: LUA_KINDS,
            bridge_query: None,
            bridge_edge_kind: "CALLS",
        }),
        _ => None,
    }
}

// =============================================================================
// Rust
// =============================================================================
const RUST_DEF_QUERY: &str = r#"
(function_item name: (identifier) @name) @def
(struct_item name: (type_identifier) @name) @def
(enum_item name: (type_identifier) @name) @def
(trait_item name: (type_identifier) @name) @def
(impl_item type: (type_identifier) @name) @def
(mod_item name: (identifier) @name) @def
(type_item name: (type_identifier) @name) @def
(const_item name: (identifier) @name) @def
"#;
const RUST_KINDS: &[&str] = &["function","struct","enum","trait","impl","module","type_alias","const"];
const RUST_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @name) @call
(call_expression function: (field_expression field: (field_identifier) @name)) @call
(call_expression function: (scoped_identifier name: (identifier) @name)) @call
(macro_invocation macro: (identifier) @name) @call
"#;

// =============================================================================
// TypeScript (pure .ts files - LANGUAGE_TYPESCRIPT, no JSX)
// =============================================================================
// A3 fix: class_declaration and interface_declaration use type_identifier (not identifier)
// in tree-sitter-typescript >= 0.23. Using identifier causes "Impossible pattern" error.
const TS_DEF_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def
(class_declaration name: (type_identifier) @name) @def
(method_definition name: (property_identifier) @name) @def
(interface_declaration name: (type_identifier) @name) @def
(type_alias_declaration name: (type_identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(lexical_declaration (variable_declarator name: (identifier) @name value: (arrow_function))) @def
"#;
const TS_KINDS: &[&str] = &["function","class","method","interface","type_alias","enum","const"];
const TS_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @name) @call
(call_expression function: (member_expression property: (property_identifier) @name)) @call
"#;

// =============================================================================
// TSX / JSX (.tsx, .jsx, .js - LANGUAGE_TSX, JSX-aware grammar)
// =============================================================================
// A2 fix: TSX grammar does NOT have a bare `function` node type.
// Use `arrow_function` and `function_expression` instead.
// `function_declaration` is still valid for named top-level functions.
const TSX_DEF_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def
(class_declaration name: (type_identifier) @name) @def
(method_definition name: (property_identifier) @name) @def
(interface_declaration name: (type_identifier) @name) @def
(type_alias_declaration name: (type_identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(lexical_declaration (variable_declarator
    name: (identifier) @name
    value: [(arrow_function) (function_expression)]
)) @def
"#;
const TSX_KINDS: &[&str] = &["function","class","method","interface","type_alias","enum","const"];

// =============================================================================
// Python
// =============================================================================
const PY_DEF_QUERY: &str = r#"
(function_definition name: (identifier) @name) @def
(class_definition name: (identifier) @name) @def
"#;
const PY_KINDS: &[&str] = &["function","class"];
const PY_CALL_QUERY: &str = r#"
(call function: (identifier) @name) @call
(call function: (attribute attribute: (identifier) @name)) @call
"#;

// =============================================================================
// Go
// =============================================================================
const GO_DEF_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def
(method_declaration name: (field_identifier) @name) @def
(type_spec name: (type_identifier) @name) @def
(const_spec name: (identifier) @name) @def
"#;
const GO_KINDS: &[&str] = &["function","method","type","const"];
const GO_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @name) @call
(call_expression function: (selector_expression field: (field_identifier) @name)) @call
"#;

// =============================================================================
// C  (also used as C++ fallback)
// =============================================================================
const C_DEF_QUERY: &str = r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def
(struct_specifier name: (type_identifier) @name) @def
(enum_specifier name: (type_identifier) @name) @def
(type_definition declarator: (type_identifier) @name) @def
"#;
const C_KINDS: &[&str] = &["function","struct","enum","type_alias"];
const C_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @name) @call
(call_expression function: (field_expression field: (field_identifier) @name)) @call
"#;

// =============================================================================
// C++  (proper grammar - handles templates, classes, namespaces)
// =============================================================================
const CPP_DEF_QUERY: &str = r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def
(function_definition declarator: (function_declarator
    declarator: (qualified_identifier scope: (_) name: (identifier) @name))) @def
(class_specifier name: (type_identifier) @name) @def
(struct_specifier name: (type_identifier) @name) @def
(enum_specifier name: (type_identifier) @name) @def
(namespace_definition name: (identifier) @name) @def
(function_definition declarator: (function_declarator
    declarator: (destructor_name (identifier) @name))) @def
"#;
const CPP_KINDS: &[&str] = &["function","method","class","struct","enum","namespace","destructor"];
const CPP_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @name) @call
(call_expression function: (field_expression field: (field_identifier) @name)) @call
(call_expression function: (qualified_identifier name: (identifier) @name)) @call
"#;

// =============================================================================
// Java
// =============================================================================
const JAVA_DEF_QUERY: &str = r#"
(method_declaration name: (identifier) @name) @def
(class_declaration name: (identifier) @name) @def
(interface_declaration name: (identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(constructor_declaration name: (identifier) @name) @def
"#;
const JAVA_KINDS: &[&str] = &["method","class","interface","enum","constructor"];
const JAVA_CALL_QUERY: &str = r#"
(method_invocation name: (identifier) @name) @call
"#;

// =============================================================================
// C#
// =============================================================================
const CS_DEF_QUERY: &str = r#"
(method_declaration name: (identifier) @name) @def
(class_declaration name: (identifier) @name) @def
(interface_declaration name: (identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(constructor_declaration name: (identifier) @name) @def
(property_declaration name: (identifier) @name) @def
"#;
const CS_KINDS: &[&str] = &["method","class","interface","enum","constructor","property"];
const CS_CALL_QUERY: &str = r#"
(invocation_expression function: (identifier) @name) @call
(invocation_expression function: (member_access_expression name: (identifier) @name)) @call
"#;

// =============================================================================
// Ruby
// =============================================================================
const RUBY_DEF_QUERY: &str = r#"
(method name: (identifier) @name) @def
(singleton_method name: (identifier) @name) @def
(class name: (constant) @name) @def
(module name: (constant) @name) @def
"#;
const RUBY_KINDS: &[&str] = &["method","method","class","module"];
const RUBY_CALL_QUERY: &str = r#"
(call method: (identifier) @name) @call
"#;

// =============================================================================
// PHP
// =============================================================================
const PHP_DEF_QUERY: &str = r#"
(function_definition name: (name) @name) @def
(method_declaration name: (name) @name) @def
(class_declaration name: (name) @name) @def
(interface_declaration name: (name) @name) @def
(trait_declaration name: (name) @name) @def
"#;
const PHP_KINDS: &[&str] = &["function","method","class","interface","trait"];
const PHP_CALL_QUERY: &str = r#"
(function_call_expression function: (name) @name) @call
(member_call_expression name: (name) @name) @call
"#;

// =============================================================================
// Kotlin
// =============================================================================
#[cfg(feature = "kotlin")]
const KOTLIN_DEF_QUERY: &str = r#"
(function_declaration (simple_identifier) @name) @def
(class_declaration (type_identifier) @name) @def
(object_declaration (type_identifier) @name) @def
(companion_object (type_identifier) @name) @def
(secondary_constructor) @def
"#;
#[cfg(feature = "kotlin")]
const KOTLIN_KINDS: &[&str] = &["function","class","object","companion","constructor"];
#[cfg(feature = "kotlin")]
const KOTLIN_CALL_QUERY: &str = r#"
(call_expression (simple_identifier) @name) @call
"#;

// =============================================================================
// Lua
// =============================================================================
const LUA_DEF_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def
(local_function name: (identifier) @name) @def
(assignment_statement (variable_list (dot_index_expression field: (identifier) @name)) (expression_list (function_definition))) @def
"#;
const LUA_KINDS: &[&str] = &["function","function","method"];
const LUA_CALL_QUERY: &str = r#"
(function_call name: (identifier) @name) @call
(method_call name: (identifier) @name) @call
"#;

// =============================================================================
// Swift
// =============================================================================
#[cfg(feature = "swift")]
const SWIFT_DEF_QUERY: &str = r#"
(class_declaration name: (type_identifier) @name) @def
(protocol_declaration name: (type_identifier) @name) @def
(function_declaration name: (simple_identifier) @name) @def
(enum_declaration name: (type_identifier) @name) @def
"#;
#[cfg(feature = "swift")]
const SWIFT_KINDS: &[&str] = &["class","protocol","function","enum"];
#[cfg(feature = "swift")]
const SWIFT_CALL_QUERY: &str = r#"
(call_expression function: (simple_identifier) @name) @call
(call_expression function: (member_expression property: (simple_identifier) @name)) @call
"#;

// =============================================================================
// Dart
// =============================================================================
#[cfg(feature = "dart")]
const DART_DEF_QUERY: &str = r#"
(class_declaration name: (identifier) @name) @def
(mixin_declaration (identifier) @name) @def
(extension_declaration name: (identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(function_signature name: (identifier) @name) @def
(method_signature (function_signature name: (identifier) @name)) @def
"#;
#[cfg(feature = "dart")]
const DART_KINDS: &[&str] = &["class","mixin","extension","enum","function","method"];
#[cfg(feature = "dart")]
const DART_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @name) @call
"#;

// =============================================================================
// BRIDGE QUERIES (Approach 1 — Built-in framework pattern detection)
// =============================================================================
//
// Captures language-specific patterns missed by def/call queries.
// Each @name capture → Deferred edge with LangConfig::bridge_edge_kind.
// Project-specific overrides → .fog-context/hints/{lang}.json (Approach 2).
// =============================================================================

// Java: @Autowired/@Inject/@Resource annotated fields → DI_INJECT edges
const JAVA_BRIDGE_QUERY: &str = r#"
(field_declaration
  (modifiers (marker_annotation name: (identifier) @_ann))
  type: (type_identifier) @name)
(field_declaration
  (modifiers (annotation name: (identifier) @_ann))
  type: (type_identifier) @name)
"#;

// Python: decorator usage → DECORATES edges
// e.g. @property, @router.get → edge to decorator name
const PY_BRIDGE_QUERY: &str = r#"
(decorated_definition
  (decorator (identifier) @name))
(decorated_definition
  (decorator (attribute attribute: (identifier) @name)))
"#;

// TypeScript/JS: dynamic import() and require() → DYNAMIC_IMPORT edges
const TS_BRIDGE_QUERY: &str = r#"
(call_expression
  function: (import)
  arguments: (arguments (string (string_fragment) @name)))
(call_expression
  function: (identifier) @_fn
  arguments: (arguments (string (string_fragment) @name)))
"#;
