//! # fog-types
//!
//! Shared type definitions for the FoG Alchemist toolchain.
//!
//! This crate is the L0 foundation - every other crate depends on it.
//! It contains ZERO logic, only data structures + serialization.
//!
//! ## Modules
//! - `error` - Unified error types
//! - `schema` - Domain schema types (Symbol, Scope, Relation)
//! - `config` - Configuration types (.fog-autoide.toml)
//! - `event` - Event bus message types
//! - `llm` - LLM request/response types
//! - `mcp` - MCP protocol types
//! - `tool` - Tool definition and result types
//! - `risk` - Risk tier and defense matrix types

pub mod config;
pub mod error;
pub mod event;
pub mod llm;
pub mod mcp;
pub mod provider;
pub mod risk;
pub mod schema;
pub mod tool;
