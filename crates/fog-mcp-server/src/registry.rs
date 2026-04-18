//! fog-mcp-server/src/registry.rs
//!
//! Multi-project registry — tracks registered workspaces.
//! Reads from ~/.fog/registry.json (same schema as TypeScript fog-context).
//!
//! PATTERN_DECISION: Level 1 (Pure function — path → registry entries)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A registered project entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub name: String,
    pub path: String,
    pub symbol_count: Option<u64>,
    pub last_indexed: Option<String>,
    pub db_path: Option<String>,
}

/// The project registry — loaded from disk on startup.
#[derive(Debug, Default)]
pub struct Registry {
    entries: Vec<RepoEntry>,
    /// The currently active project (db already open).
    pub active_project: Option<PathBuf>,
}

impl Registry {
    /// Load the global registry from `~/.fog/registry.json`.
    pub fn load() -> Self {
        let path = registry_path();
        let entries = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<Vec<RepoEntry>>(&s).ok())
                .unwrap_or_default()
        } else {
            vec![]
        };
        Self { entries, active_project: None }
    }

    /// Return all registered projects.
    pub fn list(&self) -> &[RepoEntry] {
        &self.entries
    }

    /// Find a project by path or name.
    pub fn find(&self, key: &str) -> Option<&RepoEntry> {
        self.entries.iter().find(|e| {
            e.name == key
                || e.path == key
                || e.path.ends_with(key)
        })
    }

    /// Register a new project (adds to registry.json).
    pub fn register(&mut self, name: String, path: String) {
        if !self.entries.iter().any(|e| e.path == path) {
            self.entries.push(RepoEntry {
                name,
                path,
                symbol_count: None,
                last_indexed: None,
                db_path: None,
            });
            self.save();
        }
    }

    fn save(&self) {
        let path = registry_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.entries) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn registry_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".fog").join("registry.json")
}
