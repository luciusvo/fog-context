//! fog-mcp-server/src/protocol.rs
//!
//! MCP (Model Context Protocol) JSON-RPC 2.0 type definitions.
//!
//! MCP is pure JSON-RPC 2.0 over stdio - no SDK required.
//! All types needed to decode requests and encode responses are defined here.
//!
//! Spec: https://spec.modelcontextprotocol.io/specification/
//!
//! PATTERN_DECISION: Level 1 (Pure Data Types)
//! Justification: Stateless serialization structs. No logic.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 base types
// ---------------------------------------------------------------------------

/// Incoming MCP request from the client (AI IDE).
#[derive(Debug, Deserialize)]
pub struct McpRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,   // null for notifications
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Successful MCP response.
#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub result: Value,
}

/// Error MCP response.
#[derive(Debug, Serialize)]
pub struct McpErrorResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub error: McpError,
}

/// MCP error object.
#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// ---------------------------------------------------------------------------
// MCP error codes (standard JSON-RPC + MCP extensions)
// ---------------------------------------------------------------------------

pub const ERR_PARSE:       i32 = -32700;
#[allow(dead_code)]
pub const ERR_INVALID_REQ: i32 = -32600;
pub const ERR_METHOD:      i32 = -32601;
#[allow(dead_code)]
pub const ERR_INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
pub const ERR_INTERNAL:    i32 = -32603;

// ---------------------------------------------------------------------------
// MCP tool result types
// ---------------------------------------------------------------------------

/// `tools/call` result - wraps content + isError flag.
#[derive(Debug, Serialize)]
pub struct ToolCallResult {
    pub content: Vec<TextContent>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

#[derive(Debug, Serialize)]
pub struct TextContent {
    #[serde(rename = "type")]
    pub content_type: &'static str,
    pub text: String,
}

impl TextContent {
    pub fn text(s: impl Into<String>) -> Self {
        Self { content_type: "text", text: s.into() }
    }
}

impl ToolCallResult {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            content: vec![TextContent::text(text)],
            is_error: false,
        }
    }

    #[allow(dead_code)]
    pub fn ok_json(v: &Value) -> Self {
        Self {
            content: vec![TextContent::text(serde_json::to_string_pretty(v).unwrap_or_default())],
            is_error: false,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            content: vec![TextContent::text(msg)],
            is_error: true,
        }
    }
}

// ---------------------------------------------------------------------------
// MCP Tool definition (for tools/list response)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a success response.
pub fn ok_response(id: Value, result: Value) -> Value {
    serde_json::to_value(McpResponse {
        jsonrpc: "2.0",
        id,
        result,
    }).unwrap()
}

/// Build an error response.
pub fn err_response(id: Value, code: i32, message: impl Into<String>) -> Value {
    serde_json::to_value(McpErrorResponse {
        jsonrpc: "2.0",
        id,
        error: McpError { code, message: message.into(), data: None },
    }).unwrap()
}
