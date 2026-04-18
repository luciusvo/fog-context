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
    /// Stable UUID written to <project>/.fog-id on first index.
    /// Survives folder renames and path changes.
    #[serde(default)]
    pub fog_id: Option<String>,
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

    /// Find a project by fog_id (stable UUID), path, or name.
    /// fog_id is checked first to support folder renames.
    pub fn find(&self, key: &str) -> Option<&RepoEntry> {
        // Priority: UUID > exact path > name match > path suffix
        self.entries.iter().find(|e| {
            e.fog_id.as_deref() == Some(key)
                || e.path == key
                || e.name == key
                || e.path.ends_with(key)
        })
    }

    /// Upsert a project into the registry (create if new, update if exists).
    /// Called automatically after every fog_scan.
    /// Updates symbol_count and last_indexed timestamp.
    pub fn upsert(&mut self, name: String, path: String, symbol_count: u64) {
        let fog_id = ensure_project_id(&path);
        let now = chrono_now();
        if let Some(entry) = self.entries.iter_mut().find(|e| {
            e.fog_id.as_deref() == Some(&fog_id)
                || e.path == path
        }) {
            // Update existing entry
            entry.symbol_count = Some(symbol_count);
            entry.last_indexed = Some(now);
            entry.fog_id = Some(fog_id);
        } else {
            // Create new entry
            self.entries.push(RepoEntry {
                name,
                path: path.clone(),
                symbol_count: Some(symbol_count),
                last_indexed: Some(now),
                db_path: Some(format!("{}/.fog-context/context.db", path)),
                fog_id: Some(fog_id),
            });
        }
        self.save();
    }

    /// Register a new project (legacy — does not update if already exists).
    pub fn register(&mut self, name: String, path: String) {
        self.upsert(name, path, 0);
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

/// Read existing .fog-id from a project root, if any.
pub fn read_project_id(project_path: &str) -> Option<String> {
    let id_path = std::path::Path::new(project_path).join(".fog-id");
    std::fs::read_to_string(id_path).ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Ensure .fog-id exists in the project root. Creates one if absent.
/// Returns the project UUID (a short random hex string, like "prj_3f8a2c1d").
pub fn ensure_project_id(project_path: &str) -> String {
    if let Some(id) = read_project_id(project_path) {
        return id;
    }
    let id = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let t = SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().subsec_nanos();
        format!("prj_{:08x}", t)
    };
    let id_path = std::path::Path::new(project_path).join(".fog-id");
    let _ = std::fs::write(&id_path, &id);
    id
}

/// Simple ISO-8601-like timestamp without external crates.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format: YYYY-MM-DDTHH:MM:SSZ (approximate, UTC)
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Simple date from epoch (approximate — good enough for a timestamp label)
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

