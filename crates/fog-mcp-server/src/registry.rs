//! fog-mcp-server/src/registry.rs
//!
//! Multi-project registry — tracks registered workspaces.
//! Reads from ~/.fog/registry.json (same schema as TypeScript fog-context).
//!
//! PATTERN_DECISION: Level 1 (Pure function — path → registry entries)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A registered project entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub name: String,
    pub path: String,
    pub symbol_count: Option<u64>,
    pub last_indexed: Option<String>,
    pub db_path: Option<String>,
    /// Stable UUID written to <project>/.fog-context/config.toml on first index.
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

    /// Find a project by fog_id, path, name, or fuzzy name/path-segment.
    /// Returns None if no match OR if fuzzy tier matches multiple entries (ambiguous).
    /// Priority: UUID > exact path > exact name > path suffix > name-contains > path-segment
    pub fn find(&self, key: &str) -> Option<&RepoEntry> {
        // Tier 1: fog_id exact
        if let Some(e) = self.entries.iter().find(|e| e.fog_id.as_deref() == Some(key)) {
            return Some(e);
        }
        // Tier 2: path exact
        if let Some(e) = self.entries.iter().find(|e| e.path == key) {
            return Some(e);
        }
        // Tier 3: name exact
        if let Some(e) = self.entries.iter().find(|e| e.name == key) {
            return Some(e);
        }
        // Tier 4: path ends_with (legacy suffix match)
        if let Some(e) = self.entries.iter().find(|e| e.path.ends_with(key)) {
            return Some(e);
        }
        // Tier 5: case-insensitive name contains — must be UNIQUE
        let key_lower = key.to_lowercase();
        let name_matches: Vec<_> = self.entries.iter()
            .filter(|e| e.name.to_lowercase().contains(&key_lower))
            .collect();
        match name_matches.len() {
            1 => return Some(name_matches[0]),
            0 => {}
            _ => return None, // ambiguous — caller should ESCALATE
        }
        // Tier 6: path last segment contains — must be UNIQUE
        let seg_matches: Vec<_> = self.entries.iter()
            .filter(|e| {
                std::path::Path::new(&e.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase().contains(&key_lower))
                    .unwrap_or(false)
            })
            .collect();
        match seg_matches.len() {
            1 => Some(seg_matches[0]),
            _ => None,
        }
    }

    /// Return all registered project names (for fail-loud error messages).
    pub fn list_names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
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



    /// Register a new project (legacy — does not update if already exists)
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

// ---------------------------------------------------------------------------
// Project config: .fog-context/config.toml
// ---------------------------------------------------------------------------

/// Project config stored inside .fog-context/config.toml
/// This unifies the old .fog-id and .fog.yml into a single file.
#[derive(Debug, Default)]
pub struct ProjectConfig {
    pub fog_id: Option<String>,
    pub name: Option<String>,
    pub adr_paths: Vec<String>,
    /// Version of fog-mcp-server binary that last indexed this project.
    /// Used by fog_brief to detect when binary is newer than index.
    pub indexer_version: Option<String>,
}

impl ProjectConfig {
    /// Load from .fog-context/config.toml.
    pub fn load(project_root: &Path) -> Self {
        let config_path = project_root.join(".fog-context").join("config.toml");
        if !config_path.exists() {
            return Self::default();
        }
        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        parse_config_toml(&content)
    }

    /// Save to .fog-context/config.toml.
    pub fn save(&self, project_root: &Path) {
        let dir = project_root.join(".fog-context");
        let _ = std::fs::create_dir_all(&dir);
        let mut lines = vec![
            "# .fog-context/config.toml".to_string(),
            "# Auto-generated by fog-context. Do not delete: fog_id identifies this project.".to_string(),
            String::new(),
            "[project]".to_string(),
        ];
        if let Some(ref id) = self.fog_id {
            lines.push(format!("fog_id = \"{}\"", id));
        }
        if let Some(ref name) = self.name {
            lines.push(format!("name = \"{}\"", name));
        }
        // C1: Track which binary version last indexed this project
        if let Some(ref ver) = self.indexer_version {
            lines.push(format!("indexer_version = \"{}\"", ver));
        }
        if !self.adr_paths.is_empty() {
            lines.push(String::new());
            lines.push("[adr]".to_string());
            lines.push("paths = [".to_string());
            for p in &self.adr_paths {
                lines.push(format!("  \"{}\",", p));
            }
            lines.push("]".to_string());
        }
        lines.push(String::new());
        let _ = std::fs::write(dir.join("config.toml"), lines.join("\n"));
    }
}

/// Minimal TOML parser — only handles [project] and [adr] sections we need.
/// Avoids requiring the `toml` crate as a dependency.
fn parse_config_toml(content: &str) -> ProjectConfig {
    let mut cfg = ProjectConfig::default();
    let mut in_project = false;
    let mut in_adr = false;
    let mut collecting_paths = false;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            if collecting_paths && line.is_empty() { collecting_paths = false; }
            continue;
        }
        if line == "[project]" { in_project = true; in_adr = false; continue; }
        if line == "[adr]" { in_project = false; in_adr = true; continue; }
        if line.starts_with('[') { in_project = false; in_adr = false; collecting_paths = false; continue; }

        if in_project {
            if let Some(val) = extract_toml_string(line, "fog_id") {
                cfg.fog_id = Some(val);
            } else if let Some(val) = extract_toml_string(line, "name") {
                cfg.name = Some(val);
            } else if let Some(val) = extract_toml_string(line, "indexer_version") {
                cfg.indexer_version = Some(val);
            }
        } else if in_adr {
            if line.starts_with("paths") {
                collecting_paths = true;
                // inline array: paths = ["a", "b"]
                if let Some(start) = line.find('[') {
                    let arr_str = &line[start..];
                    cfg.adr_paths = parse_toml_string_array(arr_str);
                    if !line.contains(']') { /* multi-line, continue below */ }
                    else { collecting_paths = false; }
                }
            } else if collecting_paths {
                // multi-line array entry: "path",
                if line == "]" { collecting_paths = false; continue; }
                if let Some(s) = extract_quoted_string(line) {
                    cfg.adr_paths.push(s);
                }
            }
        }
    }
    cfg
}

fn extract_toml_string(line: &str, key: &str) -> Option<String> {
    let prefix = format!("{} =", key);
    if line.starts_with(&prefix) {
        let rest = line[prefix.len()..].trim();
        extract_quoted_string(rest)
    } else {
        None
    }
}

fn extract_quoted_string(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches(',');
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        Some(s[1..s.len()-1].to_string())
    } else {
        None
    }
}

fn parse_toml_string_array(s: &str) -> Vec<String> {
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    inner.split(',')
        .filter_map(|p| extract_quoted_string(p.trim()))
        .collect()
}

// ---------------------------------------------------------------------------
// Project ID: Read from config.toml (primary) or .fog-id (legacy migration)
// ---------------------------------------------------------------------------

/// Read existing fog_id from a project root.
/// Priority: .fog-context/config.toml > .fog-id (legacy)
pub fn read_project_id(project_path: &str) -> Option<String> {
    let root = Path::new(project_path);

    // Primary: .fog-context/config.toml
    let cfg = ProjectConfig::load(root);
    if cfg.fog_id.is_some() {
        return cfg.fog_id;
    }

    // Legacy: .fog-id — read and migrate
    let id_path = root.join(".fog-id");
    if let Ok(id) = std::fs::read_to_string(&id_path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            // Migrate: write to config.toml, remove .fog-id
            let mut cfg = ProjectConfig::default();
            cfg.fog_id = Some(id.clone());
            cfg.name = root.file_name()
                .map(|n| n.to_string_lossy().into_owned());
            cfg.save(root);
            let _ = std::fs::remove_file(&id_path);
            eprintln!("[fog] Migrated .fog-id → .fog-context/config.toml");
            return Some(id);
        }
    }

    None
}

/// Ensure fog_id exists for a project. Creates .fog-context/config.toml if absent.
/// Returns the project UUID (a short random hex string, like "prj_3f8a2c1d").
pub fn ensure_project_id(project_path: &str) -> String {
    if let Some(id) = read_project_id(project_path) {
        return id; // Already exists — always reuse; fog_id is immutable
    }
    // Generate a unique fog_id without external crates.
    // Format: fog_<48-bit ms timestamp><16-bit pid><16-bit monotonic counter>
    // Example: fog_019506a8b3f812a40003
    // Collision probability: ~0 in practice (different ms + pid ensures uniqueness)
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = std::process::id();
    let cnt = COUNTER.fetch_add(1, Ordering::Relaxed);
    let id = format!("fog_{:012x}{:04x}{:04x}", ts_ms & 0xFFFF_FFFF_FFFF, pid & 0xFFFF, cnt & 0xFFFF);

    let root = Path::new(project_path);
    let mut cfg = ProjectConfig::load(root); // preserve existing name/adr_paths
    cfg.fog_id = Some(id.clone());
    if cfg.name.is_none() {
        cfg.name = root.file_name().map(|n| n.to_string_lossy().into_owned());
    }
    cfg.save(root);
    id
}

/// Load ADR paths for a project.
/// Priority: .fog-context/config.toml [adr].paths > .fog.yml > empty
pub fn load_project_adr_paths(project_root: &Path) -> Vec<String> {
    // Primary: .fog-context/config.toml
    let cfg = ProjectConfig::load(project_root);
    if !cfg.adr_paths.is_empty() {
        return cfg.adr_paths;
    }

    // Fallback: .fog.yml
    let fog_yml = project_root.join(".fog.yml");
    if let Ok(content) = std::fs::read_to_string(&fog_yml) {
        let paths: Vec<String> = content.lines()
            .skip_while(|l| !l.trim_start().starts_with("adr_paths"))
            .skip(1)
            .take_while(|l| l.trim_start().starts_with("- "))
            .map(|l| l.trim_start_matches(|c: char| c == ' ' || c == '-').trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !paths.is_empty() {
            return paths;
        }
    }

    vec![]
}

/// Simple ISO-8601-like timestamp without external crates.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}
