//! #3: Stale graph detection utility.
//!
//! Checks whether source files have changed since the last fog_scan index.
//! Uses a 3-tier fallback:
//!   1. git log --since=<last_indexed> (fastest, zero hashing)
//!   2. stat(target_file).mtime > last_indexed (O(1), no git required)
//!   3. Skipped by default (hash opt-in via verify:true in fog_impact)
//!
//! PATTERN_DECISION: Level 2 (Composition - pure functions chained)

use std::path::Path;

/// Result from a stale check.
pub enum StaleStatus {
    /// File(s) modified since last index. Include warning in response.
    Stale { files: Vec<String> },
    /// Cannot determine — no last_indexed timestamp or neither git/stat available.
    Unknown,
    /// All files up-to-date as of last index.
    Fresh,
}

/// Check whether `target_file` (relative to project root) has been modified
/// since `last_indexed` ISO timestamp.
///
/// Uses git if available, falls back to mtime stat.
pub fn check_stale(
    project_root: &Path,
    target_file: &str,
    last_indexed: Option<&str>,
) -> StaleStatus {
    let Some(since) = last_indexed else {
        return StaleStatus::Unknown;
    };

    // Tier 1: git log (most accurate, includes uncommitted changes via git status)
    if let Some(changed) = git_changed_since(project_root, since) {
        // Check if target_file appears in changed set
        let hit: Vec<String> = changed.into_iter()
            .filter(|f| f.contains(target_file) || target_file.contains(f.as_str()))
            .collect();
        if !hit.is_empty() {
            return StaleStatus::Stale { files: hit };
        }
        return StaleStatus::Fresh;
    }

    // Tier 2: mtime check on the specific file
    let abs_path = project_root.join(target_file);
    if let Ok(meta) = std::fs::metadata(&abs_path) {
        if let Ok(modified) = meta.modified() {
            // Parse last_indexed: "2026-04-19T14:00:00Z" → seconds since epoch
            if let Some(last_secs) = parse_iso_to_secs(since) {
                if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                    if dur.as_secs() > last_secs {
                        return StaleStatus::Stale {
                            files: vec![target_file.to_string()],
                        };
                    }
                    return StaleStatus::Fresh;
                }
            }
        }
    }

    StaleStatus::Unknown
}

/// Run `git log --since=<since> --name-only --pretty=format:""` in project_root.
/// Returns list of changed file paths, or None if git is not available.
fn git_changed_since(project_root: &Path, since: &str) -> Option<Vec<String>> {
    let output = std::process::Command::new("git")
        .args([
            "-C", &project_root.to_string_lossy(),
            "log",
            &format!("--since={since}"),
            "--name-only",
            "--pretty=format:",
            "--diff-filter=ACDMR",
        ])
        .output()
        .ok()?;

    if !output.status.success() { return None; }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout.lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();

    // Also check unstaged/staged changes
    let status_out = std::process::Command::new("git")
        .args(["-C", &project_root.to_string_lossy(), "status", "--porcelain"])
        .output()
        .ok()?;
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    let mut all_files = files;
    for line in status_str.lines() {
        if line.len() > 3 {
            all_files.push(line[3..].trim().to_string());
        }
    }
    all_files.dedup();
    Some(all_files)
}

/// Parse a simplified ISO-8601 timestamp to Unix seconds.
/// Handles: "2026-04-19T14:00:00Z"
fn parse_iso_to_secs(ts: &str) -> Option<u64> {
    // Very lightweight — no chrono dependency.
    // Format: YYYY-MM-DDTHH:MM:SSZ
    let ts = ts.trim_end_matches('Z');
    let parts: Vec<&str> = ts.splitn(2, 'T').collect();
    if parts.len() != 2 { return None; }
    let date_parts: Vec<u64> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    let time_parts: Vec<u64> = parts[1].split(':').filter_map(|s| s.parse().ok()).collect();
    if date_parts.len() < 3 || time_parts.len() < 3 { return None; }

    let y = date_parts[0]; let mo = date_parts[1]; let d = date_parts[2];
    let h = time_parts[0]; let m = time_parts[1]; let s = time_parts[2];
    // Approximate: 365.25 days/year, 30.44 days/month
    let days = (y.saturating_sub(1970)) * 365 + (mo.saturating_sub(1)) * 30 + d;
    Some(days * 86400 + h * 3600 + m * 60 + s)
}

/// Format a StaleStatus as a warning string to prepend to tool output.
pub fn format_warning(status: &StaleStatus, tool_name: &str) -> Option<String> {
    match status {
        StaleStatus::Stale { files } => Some(format!(
            "> [!WARNING]\n\
             > **Stale graph detected** — {} file(s) changed since last `fog_scan`:\n\
             {}\n\
             > `{tool_name}` results may be outdated.\n\
             > → Run `fog_scan({{ \"project\": \"<fog_id>\" }})` to refresh.\n\
             > → Ideal trigger: after `git commit` or major code changes.\n\n",
            files.len(),
            files.iter().map(|f| format!("> - `{f}`")).collect::<Vec<_>>().join("\n")
        )),
        _ => None,
    }
}
