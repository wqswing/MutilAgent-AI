use crate::mcp_registry::{McpRegistry, McpServerInfo};
use multi_agent_core::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;

#[derive(Deserialize)]
struct McpConfig {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Deserialize)]
struct McpServerConfig {
    command: String,
    args: Vec<String>,
}

/// Load an MCP configuration file and register servers to the registry.
pub async fn load_mcp_config(registry: Arc<McpRegistry>, path: &Path) -> Result<()> {
    if !path.exists() {
        tracing::warn!("MCP config file not found at: {:?}", path);
        return Ok(());
    }

    let content = fs::read_to_string(path)
        .await
        .map_err(|e| Error::mcp_adapter(format!("Failed to read MCP config: {}", e)))?;

    // Try parsing as TOML first, then JSON (naive approach, or rely on extension)
    let config: McpConfig = if path.extension().is_some_and(|ext| ext == "json") {
        serde_json::from_str(&content)
            .map_err(|e| Error::mcp_adapter(format!("Failed to parse MCP config (JSON): {}", e)))?
    } else {
        toml::from_str(&content)
            .map_err(|e| Error::mcp_adapter(format!("Failed to parse MCP config (TOML): {}", e)))?
    };

    for (id, server_conf) in config.mcp_servers {
        let info = McpServerInfo::new(&id, &id)
            .with_uri(server_conf.command)
            .with_args(
                server_conf
                    .args
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>(),
            )
            .with_transport("stdio");

        // TODO: Map capabilities from config if available (currently config doesn't have them)
        registry.register(info);

        // Auto-connect?
        // Let's not auto-connect to all servers to save resources.
        // Or should we?
        // The plan said "Dynamically register MCP clients". Connection might happen on demand.
        // But the previous implementation had comments about connecting.
        // Let's just register for now.
    }

    Ok(())
}
