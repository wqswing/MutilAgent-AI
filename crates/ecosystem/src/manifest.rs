use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Manifest defining a plugin's identity, capabilities, and permissions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginManifest {
    /// Unique identifier (e.g., "mcp-filesystem").
    pub id: String,
    /// Semantic version (e.g., "1.0.0").
    pub version: String,
    /// Human-readable name.
    pub name: String,
    /// Description of functionality.
    pub description: String,
    /// List of MCP capabilities provided.
    pub capabilities: Vec<String>,
    /// Requested permissions.
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    /// Risk declaration for tools.
    #[serde(default)]
    pub risk_declaration: RiskDeclaration,
    /// Transport configuration for connecting to the server.
    pub transport: PluginTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginTransport {
    /// Type of transport: "stdio", "sse", "websocket".
    #[serde(default = "default_transport_type")]
    pub r#type: String,
    /// Command to execute (for stdio).
    pub command: Option<String>,
    /// Arguments for the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// URL (for sse/websocket).
    pub url: Option<String>,
}

fn default_transport_type() -> String {
    "stdio".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginPermission {
    /// Resource type (e.g., "filesystem", "network").
    pub resource: String,
    /// Access mode or details (e.g., "/tmp:read-write", "github.com").
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RiskDeclaration {
    /// Default risk level for tools in this plugin.
    pub default_risk: String, // "low", "medium", "high", "critical"
    /// Specific risk rules.
    #[serde(default)]
    pub rules: Vec<RiskRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskRule {
    /// Pattern to match tool name or arguments.
    pub pattern: String,
    /// Risk level if matched.
    pub risk: String,
    /// Reason for the risk level.
    pub reason: String,
}

impl PluginManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read manifest at {:?}", path.as_ref()))?;
        serde_yaml::from_str(&content).with_context(|| "Failed to parse plugin manifest YAML")
    }
}
