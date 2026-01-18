//! MCP (Model Context Protocol) adapter for connecting to external tool servers.
//!
//! This module provides an adapter for the MCP protocol, allowing Multiagent to
//! connect to external MCP servers and use their tools as if they were local.

use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use multi_agent_core::{
    types::{ToolDefinition, ToolOutput},
    Error, Result,
};

/// MCP transport type for connecting to servers.
#[derive(Debug, Clone)]
pub enum McpTransport {
    /// Connect via standard IO (subprocess)
    Stdio {
        /// Command to run
        command: String,
        /// Command arguments
        args: Vec<String>,
    },
    /// Connect via Server-Sent Events
    Sse {
        /// SSE endpoint URL
        url: String,
    },
    /// Connect via WebSocket
    WebSocket {
        /// WebSocket URL
        url: String,
    },
}

/// MCP server connection state.
#[derive(Debug)]
pub struct McpServerConnection {
    /// Server name
    pub name: String,
    /// Transport configuration
    pub transport: McpTransport,
    /// Whether currently connected
    pub connected: bool,
    /// Available tools from this server
    pub tools: Vec<ToolDefinition>,
}

/// MCP tool adapter for managing connections to MCP servers.
///
/// This adapter allows Multiagent to:
/// - Connect to multiple MCP servers
/// - Discover tools from each server
/// - Execute tools on remote servers
pub struct McpToolAdapter {
    /// Connected servers
    servers: DashMap<String, Arc<RwLock<McpServerConnection>>>,
}

impl Default for McpToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl McpToolAdapter {
    /// Create a new MCP tool adapter.
    pub fn new() -> Self {
        Self {
            servers: DashMap::new(),
        }
    }

    /// Connect to an MCP server.
    ///
    /// # Arguments
    /// * `name` - A unique name for this server connection
    /// * `transport` - The transport configuration
    ///
    /// # Example
    /// ```ignore
    /// adapter.connect("local-tools", McpTransport::Stdio {
    ///     command: "npx".to_string(),
    ///     args: vec!["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    /// }).await?;
    /// ```
    pub async fn connect(&self, name: &str, transport: McpTransport) -> Result<()> {
        tracing::info!(server = %name, transport = ?transport, "Connecting to MCP server");

        // Create connection state
        let connection = McpServerConnection {
            name: name.to_string(),
            transport: transport.clone(),
            connected: false,
            tools: Vec::new(),
        };

        // Store connection (actual MCP connection would happen here)
        self.servers.insert(
            name.to_string(),
            Arc::new(RwLock::new(connection)),
        );

        // In a full implementation, we would:
        // 1. Spawn the transport (subprocess, HTTP client, WebSocket client)
        // 2. Send initialize request
        // 3. Receive capabilities and tool list
        // 4. Store the tools

        // For now, mark as connected (mock)
        if let Some(server) = self.servers.get(name) {
            let mut conn = server.write().await;
            conn.connected = true;
            
            // Mock: Add some placeholder tools
            conn.tools.push(ToolDefinition {
                name: format!("{}/list_files", name),
                description: "List files in a directory".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Directory path"}
                    },
                    "required": ["path"]
                }),
                supports_streaming: false,
            });
        }

        tracing::info!(server = %name, "MCP server connected (mock)");
        Ok(())
    }

    /// Disconnect from an MCP server.
    pub async fn disconnect(&self, name: &str) -> Result<()> {
        if let Some((_, server)) = self.servers.remove(name) {
            let mut conn = server.write().await;
            conn.connected = false;
            tracing::info!(server = %name, "MCP server disconnected");
        }
        Ok(())
    }

    /// List all connected servers.
    pub fn list_servers(&self) -> Vec<String> {
        self.servers.iter().map(|e| e.key().clone()).collect()
    }

    /// Helper to clone self reference if wrapped in Arc (trickier from &self inside impl)
    /// Actually, usage inside McpRegistry usually holds the Arc. 
    /// But get_tool is called on &self.
    /// WE NEED TO CHANGE get_tool signature or usage? No, we can't change ToolRegistry trait easily.
    /// BUT McpRegistry struct holds `adapter: Arc<McpToolAdapter>`.
    /// When McpRegistry calls `self.adapter.get_tool()`, it's calling on `Arc<McpToolAdapter>`.
    /// So `get_tool` has `&self`. We can't upgrade `&self` to `Arc<Self>` easily unless we use a weak ref or passed arc.
    ///
    /// ALTERNATIVE: `get_tool` on Adapter shouldn't return `Box<dyn Tool>`.
    /// It should return `Option<ToolDefinition>`.
    /// And `McpRegistry` (which HOLDS the Arc) constructs the `McpToolWrapper`.
    ///
    /// Let's REVERT the logic in `get_tool` to return `Option<ToolDefinition>` 
    /// and move the wrapper construction to `McpRegistry`!
    ///
    /// Wait, `get_tool` signature in `McpToolAdapter` I just added returns `Result<Option<Box<dyn Tool>>>`.
    /// Let's change it to return `Result<Option<ToolDefinition>>`.
    pub async fn get_tool_definition(&self, name: &str) -> Result<Option<ToolDefinition>> {
        // 1. Try exact match (server/tool)
        if name.contains('/') {
             let parts: Vec<&str> = name.splitn(2, '/').collect();
             let server_name = parts[0];
             
             if let Some(server) = self.servers.get(server_name) {
                 let conn = server.read().await;
                 if conn.connected {
                     if let Some(def) = conn.tools.iter().find(|t| t.name == name) {
                         return Ok(Some(def.clone()));
                     }
                 }
             }
        } else {
            // Search all servers
            for entry in self.servers.iter() {
                let conn = entry.value().read().await;
                if conn.connected {
                     if let Some(def) = conn.tools.iter().find(|t| t.name.ends_with(name) || t.name == name) {
                           return Ok(Some(def.clone()));
                     }
                }
            }
        }
        Ok(None)
    }

    /// List all tools from all connected servers.
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        let mut all_tools = Vec::new();
        
        for entry in self.servers.iter() {
            let conn = entry.value().read().await;
            if conn.connected {
                all_tools.extend(conn.tools.clone());
            }
        }
        
        Ok(all_tools)
    }

    /// Get tools from a specific server.
    pub async fn get_server_tools(&self, server_name: &str) -> Result<Vec<ToolDefinition>> {
        if let Some(server) = self.servers.get(server_name) {
            let conn = server.read().await;
            if conn.connected {
                return Ok(conn.tools.clone());
            } else {
                return Err(Error::mcp_adapter(format!("Server '{}' is not connected", server_name)));
            }
        }
        Err(Error::mcp_adapter(format!("Server '{}' not found", server_name)))
    }



    /// Find a tool definition by name.
    pub async fn find_tool(&self, name: &str) -> Option<ToolDefinition> {
         if name.contains('/') {
             let parts: Vec<&str> = name.splitn(2, '/').collect();
             let server_name = parts[0];
             // let tool_name = parts[1]; // unused
             
             if let Some(server) = self.servers.get(server_name) {
                 let conn = server.read().await;
                 if conn.connected {
                     if let Some(def) = conn.tools.iter().find(|t| t.name == name) {
                         return Some(def.clone());
                     }
                 }
             }
        }
        // Fallback: search all
        for entry in self.servers.iter() {
            let conn = entry.value().read().await;
            if conn.connected {
                 if let Some(def) = conn.tools.iter().find(|t| t.name == name) {
                     return Some(def.clone());
                 }
            }
        }
        None
    }

    /// Call a tool on an MCP server.
    ///
    /// Tool names are in the format "server_name/tool_name".
    pub async fn call_tool(
        &self,
        full_tool_name: &str,
        args: serde_json::Value,
    ) -> Result<ToolOutput> {
        // Parse server/tool name
        let parts: Vec<&str> = full_tool_name.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(Error::mcp_adapter(format!(
                "Invalid tool name format. Expected 'server/tool', got '{}'",
                full_tool_name
            )));
        }

        let server_name = parts[0];
        let tool_name = parts[1];

        tracing::info!(server = %server_name, tool = %tool_name, "Calling MCP tool");

        // Check if server exists and is connected
        let server = self.servers.get(server_name).ok_or_else(|| {
            Error::mcp_adapter(format!("MCP server '{}' not found", server_name))
        })?;

        let conn = server.read().await;
        if !conn.connected {
            return Err(Error::mcp_adapter(format!(
                "MCP server '{}' is not connected",
                server_name
            )));
        }

        // Check if tool exists
        let tool_exists = conn.tools.iter().any(|t| t.name == full_tool_name);
        if !tool_exists {
            return Err(Error::mcp_adapter(format!(
                "Tool '{}' not found on server '{}'",
                tool_name, server_name
            )));
        }

        // In a full implementation, we would:
        // 1. Send tool call request to the MCP server
        // 2. Wait for response
        // 3. Parse and return result

        // Mock response
        Ok(ToolOutput::text(format!(
            "MCP tool '{}' executed with args: {}. (Mock response - real MCP integration pending)",
            full_tool_name,
            serde_json::to_string_pretty(&args).unwrap_or_default()
        )))
    }

    /// Check if a tool name is an MCP tool (contains '/').
    pub fn is_mcp_tool(tool_name: &str) -> bool {
        tool_name.contains('/')
    }
}

/// MCP tool that wraps the adapter for use in the tool registry.
pub struct McpTool {
    adapter: Arc<McpToolAdapter>,
}

impl McpTool {
    /// Create a new MCP tool wrapper.
    pub fn new(adapter: Arc<McpToolAdapter>) -> Self {
        Self { adapter }
    }
}

/// A wrapper for a specific MCP tool returned by get_tool.
pub struct McpToolWrapper {
    pub adapter: Arc<McpToolAdapter>,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[async_trait]
impl multi_agent_core::traits::Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput> {
        self.adapter.call_tool(&self.name, args).await
    }
}

#[async_trait]
impl multi_agent_core::traits::Tool for McpTool {
    fn name(&self) -> &str {
        "mcp"
    }

    fn description(&self) -> &str {
        "Execute tools on connected MCP servers"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "MCP server name"
                },
                "tool": {
                    "type": "string",
                    "description": "Tool name on the server"
                },
                "args": {
                    "type": "object",
                    "description": "Tool arguments"
                }
            },
            "required": ["server", "tool"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput> {
        let server = args["server"].as_str().ok_or_else(|| {
            Error::mcp_adapter("Missing 'server' parameter".to_string())
        })?;
        
        let tool = args["tool"].as_str().ok_or_else(|| {
            Error::mcp_adapter("Missing 'tool' parameter".to_string())
        })?;
        
        let tool_args = args.get("args").cloned().unwrap_or(serde_json::json!({}));
        
        let full_name = format!("{}/{}", server, tool);
        self.adapter.call_tool(&full_name, tool_args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_and_list() {
        let adapter = McpToolAdapter::new();
        
        adapter.connect("test-server", McpTransport::Stdio {
            command: "echo".to_string(),
            args: vec![],
        }).await.unwrap();

        let servers = adapter.list_servers();
        assert!(servers.contains(&"test-server".to_string()));

        let tools = adapter.list_tools().await.unwrap();
        assert!(!tools.is_empty());
    }

    #[tokio::test]
    async fn test_call_tool() {
        let adapter = McpToolAdapter::new();
        
        adapter.connect("fs", McpTransport::Sse {
            url: "http://localhost:8080".to_string(),
        }).await.unwrap();

        let result = adapter.call_tool(
            "fs/list_files",
            serde_json::json!({"path": "/tmp"}),
        ).await.unwrap();

        assert!(result.success);
        assert!(result.content.contains("list_files"));
    }

    #[test]
    fn test_is_mcp_tool() {
        assert!(McpToolAdapter::is_mcp_tool("server/tool"));
        assert!(!McpToolAdapter::is_mcp_tool("local_tool"));
    }
}
