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

    /// Register a new project. Creates .fog-id in the project root on first index.
    pub fn register(&mut self, name: String, path: String) {
        let fog_id = ensure_project_id(&path);
        if !self.entries.iter().any(|e| e.path == path) {
            self.entries.push(RepoEntry {
                name,
                path,
                symbol_count: None,
                last_indexed: None,
                db_path: None,
                fog_id: Some(fog_id),
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
    // Generate a simple 8-hex random ID without external crate
    let id = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let t = SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().subsec_nanos();
        format!("prj_{:08x}", t)
    };
    let id_path = std::path::Path::new(project_path).join(".fog-id");
    let _ = std::fs::write(&id_path, &id);
    tracing::info!("fog-context: created .fog-id = {id} at {project_path}");
    id
}
