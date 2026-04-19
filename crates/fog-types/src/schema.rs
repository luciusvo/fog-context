//! Domain schema types for code intelligence.
//!
//! These types model the code structure stored in fog-core's SQLite database.
//! They are the "nouns" of the system: files, symbols, scopes, relations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

/// Supported programming languages for Tree-sitter parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    TypeScript,
    Python,
    Rust,
}

impl Language {
    /// Determine language from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "ts" | "tsx" | "js" | "jsx" => Some(Self::TypeScript),
            "py" => Some(Self::Python),
            "rs" => Some(Self::Rust),
            _ => None,
        }
    }

    /// Canonical file extensions for this language.
    pub fn extensions(&self) -> &[&str] {
        match self {
            Self::TypeScript => &["ts", "tsx", "js", "jsx"],
            Self::Python => &["py"],
            Self::Rust => &["rs"],
        }
    }
}

// ---------------------------------------------------------------------------
// Symbol
// ---------------------------------------------------------------------------

/// A code symbol extracted by Tree-sitter (function, class, type, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: Uuid,
    pub file_path: String,
    pub name: String,
    pub kind: SymbolKind,
    pub language: Language,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    /// Optional: the full signature / prototype text
    pub signature: Option<String>,
    /// Optional: docstring or comment above the symbol
    pub doc_comment: Option<String>,
    pub indexed_at: DateTime<Utc>,
}

/// Kind of code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    TypeAlias,
    Constant,
    Variable,
    Module,
    Trait,
    Impl,
}

// ---------------------------------------------------------------------------
// IndexedFile
// ---------------------------------------------------------------------------

/// Metadata about an indexed source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    pub path: String,
    pub language: Language,
    pub size_bytes: u64,
    pub line_count: u32,
    pub symbol_count: u32,
    pub content_hash: String,
    pub indexed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Relation (dependency graph edges)
// ---------------------------------------------------------------------------

/// A directed edge in the dependency/call graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: Uuid,
    pub from_symbol: Uuid,
    pub to_symbol: Uuid,
    pub kind: RelationKind,
}

/// Type of relationship between two symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Calls,
    Imports,
    Extends,
    Implements,
    UsesType,
}

// ---------------------------------------------------------------------------
// Domain Entity (from GLOSSARY.md)
// ---------------------------------------------------------------------------

/// A domain entity parsed from the project's GLOSSARY.md.
/// Used for Zero-Trust anchoring - AI can only use terms defined here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEntity {
    pub name: String,
    pub definition: String,
    pub category: DomainCategory,
    /// Which symbols in the codebase implement this entity
    pub linked_symbols: Vec<Uuid>,
}

/// Category of domain entity (from DDD taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainCategory {
    Entity,
    ValueObject,
    Aggregate,
    Command,
    Event,
    Service,
    Repository,
}

// ---------------------------------------------------------------------------
// Search result
// ---------------------------------------------------------------------------

/// Result from FTS5 BM25 search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub symbol: Symbol,
    pub rank: f64,
    pub snippet: String,
}
