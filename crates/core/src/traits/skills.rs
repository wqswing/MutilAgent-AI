//! L2 Skills traits.

use async_trait::async_trait;
use serde_json::Value;
use crate::error::Result;
use crate::types::{ToolDefinition, ToolOutput};

/// Tool interface for atomic operations.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the unique name of the tool.
    fn name(&self) -> &str;

    /// Get the human-readable description.
    fn description(&self) -> &str;

    /// Get the JSON Schema for parameters.
    fn parameters(&self) -> Value;

    /// Execute the tool with the given arguments.
    async fn execute(&self, args: Value) -> Result<ToolOutput>;

    /// Get the risk level of this tool for HITL approval gating.
    /// Override this for tools that modify state or execute code.
    fn risk_level(&self) -> crate::types::ToolRiskLevel {
        crate::types::ToolRiskLevel::Low
    }
}

/// Tool registry for managing available tools.
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    /// Register a new tool.
    async fn register(&self, tool: Box<dyn Tool>) -> Result<()>;

    /// Get a tool by name.
    async fn get(&self, name: &str) -> Result<Option<Box<dyn Tool>>>;

    /// List all available tools.
    async fn list(&self) -> Result<Vec<ToolDefinition>>;

    /// Execute a tool by name with arguments.
    async fn execute(&self, name: &str, args: Value) -> Result<ToolOutput>;

    /// Get the risk level of a tool by name.
    /// Returns `Low` if the tool is not found.
    async fn get_risk_level(&self, name: &str) -> crate::types::ToolRiskLevel {
        match self.get(name).await {
            Ok(Some(tool)) => tool.risk_level(),
            _ => crate::types::ToolRiskLevel::Low,
        }
    }
}

/// MCP (Model Context Protocol) adapter.
#[async_trait]
pub trait McpAdapter: Send + Sync {
    /// Connect to an MCP server.
    async fn connect(&mut self, server_url: &str) -> Result<()>;

    /// Disconnect from the MCP server.
    async fn disconnect(&mut self) -> Result<()>;

    /// List available tools from the MCP server.
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>>;

    /// Execute a tool on the MCP server.
    async fn execute_tool(&self, name: &str, args: Value) -> Result<ToolOutput>;
}
