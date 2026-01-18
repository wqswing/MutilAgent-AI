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
pub mod mcp_adapter;
pub mod mcp_registry;
pub mod registry;
pub mod composite_registry;

pub use builtin::*;
pub use code_simplifier::{simplify_rust_code, SimplifiedCode};
pub use mcp_adapter::{McpToolAdapter, McpTransport, McpTool};
pub use mcp_registry::{McpRegistry, McpServerInfo, McpCapability};
pub use registry::DefaultToolRegistry;
pub use composite_registry::CompositeToolRegistry;
