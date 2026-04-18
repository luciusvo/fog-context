//! Tool definition and result types for the built-in tool dispatcher.

use serde::{Deserialize, Serialize};

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub success: bool,
    pub output: String,
    /// If the tool produced structured data, it goes here.
    pub data: Option<serde_json::Value>,
}

/// Built-in tools available in every fog task session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum BuiltinTool {
    // -- File operations (Codex DNA) --
    ReadFile {
        path: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
    },
    WriteFile {
        path: String,
        content: String,
    },
    EditFile {
        path: String,
        old_text: String,
        new_text: String,
    },
    ListDirectory {
        path: String,
        #[serde(default)]
        recursive: bool,
    },

    // -- Search (Zed DNA) --
    SearchText {
        query: String,
        path: Option<String>,
        #[serde(default)]
        regex: bool,
    },
    SearchFiles {
        pattern: String,
    },

    // -- FoG semantic tools (fog-core powered) --
    FogQuerySemantic {
        query: String,
        limit: Option<u32>,
    },
    FogGetImpactGraph {
        symbol_id: String,
        depth: Option<u32>,
    },
    FogGetDomainCatalog {
        filter: Option<String>,
    },

    // -- Session control --
    TaskComplete {
        summary: String,
    },
}

impl BuiltinTool {
    /// Tool name as string (for matching LLM tool calls).
    pub fn name(&self) -> &'static str {
        match self {
            Self::ReadFile { .. } => "read_file",
            Self::WriteFile { .. } => "write_file",
            Self::EditFile { .. } => "edit_file",
            Self::ListDirectory { .. } => "list_directory",
            Self::SearchText { .. } => "search_text",
            Self::SearchFiles { .. } => "search_files",
            Self::FogQuerySemantic { .. } => "fog_query_semantic",
            Self::FogGetImpactGraph { .. } => "fog_get_impact_graph",
            Self::FogGetDomainCatalog { .. } => "fog_get_domain_catalog",
            Self::TaskComplete { .. } => "task_complete",
        }
    }
}
