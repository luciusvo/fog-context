use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use regex::RegexBuilder;
use serde::Deserialize;
use serde_json::Value;

use crate::protocol::{ToolCallResult, ToolDef, TextContent};

// PATTERN_DECISION: Level 1 (Pure Function)
// Justification: Raw text search is a purely functional transformation from input path + query to output matches.
// No mutable or shared state required beyond local filesystem reading.

#[derive(Deserialize)]
pub struct SearchArgs {
    pub query: String,
    pub path: Option<String>,
    pub includes: Option<Vec<String>>,
    #[serde(default)] pub is_regex: bool,
    #[serde(default)] pub context_lines: u8,
    #[serde(default)] pub case_sensitive: bool,
    #[allow(dead_code)] pub project: Option<String>,
}

pub fn definition() -> ToolDef {
    ToolDef {
        name: "fog_search".into(),
        description: "Raw text and regex search across the project files (max 20 exact matches or a distribution summary if > 50). Use this when semantic search (fog_lookup) fails to find exact literal strings or specific syntax patterns.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The text or regex to search for." },
                "path": { "type": "string", "description": "Optional sub-directory or file path to constrain search." },
                "includes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional glob patterns (e.g. '*.rs', '*.ts')."
                },
                "is_regex": { "type": "boolean", "description": "If true, treat query as a Regex." },
                "context_lines": { "type": "integer", "description": "Number of lines of context before/after matches (default 0, max 10)." },
                "case_sensitive": { "type": "boolean", "description": "If true, search is case sensitive (default false)." },
                "project": { "type": "string", "description": "Project ID/Path." }
            },
            "required": ["query"]
        }),
    }
}

pub fn handle(args: &Value, project_root: &Path) -> ToolCallResult {
    let parsed: SearchArgs = match serde_json::from_value(args.clone()) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::err(format!("Invalid args: {e}")),
    };
    run_search(parsed, project_root)
}

struct SearchMatch {
    file: String,
    line_num: usize,
    lines: Vec<(usize, String)>, // (line_num, text)
}

fn run_search(args: SearchArgs, project_root: &Path) -> ToolCallResult {
    // 1. Resolve path and sandbox (Security/Zero-Trust)
    let search_root = if let Some(p) = &args.path {
        let p_path = Path::new(p);
        let joined = project_root.join(p_path);
        // Canonicalization protects against traversal like ../../etc/shadow
        match joined.canonicalize() {
            Ok(c) => {
                if !c.starts_with(project_root) {
                    return ToolCallResult::err("Path must be inside project root".to_string());
                }
                c
            }
            Err(_) => return ToolCallResult::err(format!("Path does not exist: {}", p)),
        }
    } else {
        project_root.to_path_buf()
    };

    // 2. Build Regex
    let pattern = if args.is_regex {
        args.query.clone()
    } else {
        regex::escape(&args.query)
    };

    let re = match RegexBuilder::new(&pattern)
        .case_insensitive(!args.case_sensitive)
        .build()
    {
        Ok(r) => r,
        Err(e) => return ToolCallResult::err(format!("Invalid regex: {e}")),
    };

    // 3. Build file walker
    let mut builder = ignore::WalkBuilder::new(&search_root);
    builder.hidden(true).ignore(true).git_ignore(true);

    if let Some(includes) = &args.includes {
        let mut ov = ignore::overrides::OverrideBuilder::new(&search_root);
        for inc in includes {
            if let Err(e) = ov.add(inc) {
                return ToolCallResult::err(format!("Invalid include glob: {inc} - {e}"));
            }
        }
        match ov.build() {
            Ok(o) => { builder.overrides(o); },
            Err(e) => return ToolCallResult::err(format!("Error building overrides: {e}")),
        }
    }

    let mut matches = Vec::new();
    let mut total_matches = 0;
    let mut dir_distribution: HashMap<String, usize> = HashMap::new();
    let context_lines = args.context_lines.min(10) as usize; // Cap at 10

    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();

        // [HIGH] CONSTRAINT: Skip large files and minified bundles
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        if file_name.ends_with(".min.js") || file_name.ends_with(".min.css") || file_name.ends_with(".map") || file_name.ends_with(".svg") {
            continue;
        }

        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.len() > 1_000_000 { // 1MB limit
                continue;
            }
        } else {
            continue;
        }

        // Search within file
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        let mut file_matched = false;

        for (idx, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                total_matches += 1;
                file_matched = true;

                if matches.len() < 20 {
                    let start = idx.saturating_sub(context_lines);
                    let end = (idx + context_lines).min(lines.len().saturating_sub(1));
                    
                    let mut context = Vec::new();
                    for i in start..=end {
                        context.push((i + 1, lines[i].clone()));
                    }

                    let rel_path = path.strip_prefix(project_root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned();

                    matches.push(SearchMatch {
                        file: rel_path,
                        line_num: idx + 1,
                        lines: context,
                    });
                }
            }
        }

        if file_matched {
            let rel_path = path.strip_prefix(project_root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            
            // Group by directory for distribution summary
            let dir = std::path::Path::new(&rel_path)
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy()
                .into_owned();
            
            let dir_name = if dir.is_empty() { ".".to_string() } else { dir };
            *dir_distribution.entry(dir_name).or_insert(0) += 1;
        }
    }

    if total_matches == 0 {
        return ToolCallResult {
            content: vec![TextContent::text("No matches found.")],
            is_error: false,
        };
    }

    let mut output = String::new();

    if total_matches > 50 {
        output.push_str(&format!("Found {} matches. Returning Distribution Summary to prevent context overflow:\n\n", total_matches));
        output.push_str("📊 Distribution (Files with matches per directory):\n");
        let mut dirs: Vec<_> = dir_distribution.iter().collect();
        dirs.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending
        for (dir, count) in dirs {
            output.push_str(&format!("  - {}: {} file(s)\n", dir, count));
        }
    } else {
        output.push_str(&format!("Found {} matches:\n\n", total_matches));
        for m in matches {
            output.push_str(&format!("File: {}\n", m.file));
            for (num, line) in m.lines {
                let prefix = if num == m.line_num { ">" } else { " " };
                output.push_str(&format!("{} {:5} | {}\n", prefix, num, line));
            }
            output.push_str("\n");
        }
    }

    ToolCallResult {
        content: vec![TextContent::text(output)],
        is_error: false,
    }
}
