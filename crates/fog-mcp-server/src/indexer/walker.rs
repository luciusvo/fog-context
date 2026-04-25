//! fog-mcp-server/src/indexer/walker.rs
//!
//! Gitignore-aware file walker using the `ignore` crate (same as ripgrep).
//! Filters to supported languages only.

use std::path::Path;
use ignore::WalkBuilder;

use super::langs::lang_for_extension;

/// A scanned source file ready for Tree-sitter parsing.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Path relative to project root (matches what goes into files.path in DB)
    pub path: String,
    /// Detected language name (e.g., "rust", "typescript", "python")
    pub lang: &'static str,
    /// XXH3 checksum of file content (for incremental indexing)
    pub checksum: String,
    /// Number of lines
    pub line_count: u32,
    /// File size in bytes
    pub size_bytes: u64,
}

/// Walk the project directory and return all indexable source files.
///
/// Respects:
/// - `.gitignore` / `.ignore` / `.fogignore`
/// - Hidden directories (skipped)
/// - `node_modules/`, `target/`, `dist/` (auto-excluded by `ignore` crate)
///
/// PATTERN_DECISION: Level 1 (Pure Function - input path → output file list)
pub fn walk_project(root: &Path) -> Vec<ScannedFile> {
    let mut files = Vec::new();

    let fogignore = root.join(".fogignore");
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .ignore(true);                          // respects .ignore files

    // If .fogignore exists, add it as an extra custom ignore file
    if fogignore.exists() {
        builder.add_ignore(&fogignore);
    }

    let walker = builder
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(name.as_ref(),
                "node_modules" | "target"    | "dist"   | "build"
                | ".fog-context" | ".git"    | ".venv"  | "venv"
                | "__pycache__"  | ".cache"  | ".next"  | ".nuxt"
                | "vendor"       | "tmp"     | "temp"   | ".tmp"
                | "coverage"     | ".nyc_output"
            )
        })
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Only process files (not dirs)
        if entry.file_type().map(|t| !t.is_file()).unwrap_or(true) {
            continue;
        }

        let path = entry.path();
        let Some(lang) = path.extension()
            .and_then(|e| e.to_str())
            .and_then(lang_for_extension) else {
            continue; // skip unsupported languages
        };

        // Relative path from project root
        let rel_path = match path.strip_prefix(root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        // Read for checksum
        let content = match std::fs::read(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let size_bytes = content.len() as u64;
        let checksum = compute_checksum(&content);
        let line_count = content.iter().filter(|&&b| b == b'\n').count() as u32;

        files.push(ScannedFile {
            path: rel_path.to_string(),
            lang,
            checksum,
            line_count,
            size_bytes,
        });
    }

    files
}

/// Compute XXH3-128 checksum of file bytes, returned as hex string.
fn compute_checksum(content: &[u8]) -> String {
    use xxhash_rust::xxh3::xxh3_64;
    format!("{:016x}", xxh3_64(content))
}
