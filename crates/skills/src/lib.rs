#![deny(unused)]
//! L2 Skills & Workers for Multiagent.
//!
//! This crate provides:
//! - Tool registry for managing available tools
//! - Built-in tools (read_artifact, echo, etc.)
//! - Code simplifier for AST-based skeletonization
//! - MCP adapter for external tool servers

pub mod builtin;
pub mod code_simplifier;
pub mod composite_registry;
pub mod loader;
pub mod mcp_adapter;
pub mod mcp_registry;
pub mod network;
pub mod registry;

pub use builtin::*;
pub use code_simplifier::{simplify_rust_code, SimplifiedCode};
pub use composite_registry::CompositeToolRegistry;
pub use loader::load_mcp_config;
pub use mcp_adapter::{McpTool, McpToolAdapter, McpTransport};
pub use mcp_registry::{McpCapability, McpRegistry, McpServerInfo};
pub use registry::DefaultToolRegistry;
