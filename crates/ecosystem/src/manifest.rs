use anyhow::{Context, Result};
use semver::Version;
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
    /// Distribution channel for rollout governance.
    #[serde(default = "default_distribution_channel")]
    pub distribution_channel: String,
    /// Minimum runtime version required (semver).
    #[serde(default)]
    pub min_runtime_version: Option<String>,
    /// Optional supply-chain signature digest.
    #[serde(default)]
    pub signature: Option<String>,
    /// Transport configuration for connecting to the server.
    pub transport: PluginTransport,
}

fn default_distribution_channel() -> String {
    "stable".to_string()
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

    pub fn validate_for_runtime(&self, runtime_version: &str) -> Result<()> {
        let plugin_version = Version::parse(&self.version)
            .with_context(|| format!("Plugin '{}' has invalid semver", self.id))?;
        let _ = plugin_version;

        if !matches!(self.distribution_channel.as_str(), "stable" | "canary") {
            anyhow::bail!(
                "Plugin '{}' uses unsupported distribution channel '{}'",
                self.id,
                self.distribution_channel
            );
        }

        let runtime = Version::parse(runtime_version)
            .with_context(|| format!("Runtime version '{}' is invalid semver", runtime_version))?;

        if let Some(min_runtime) = &self.min_runtime_version {
            let min = Version::parse(min_runtime).with_context(|| {
                format!(
                    "Plugin '{}' has invalid min_runtime_version '{}'",
                    self.id, min_runtime
                )
            })?;
            if runtime < min {
                anyhow::bail!(
                    "Plugin '{}' requires runtime >= {}, got {}",
                    self.id,
                    min,
                    runtime
                );
            }
        }

        if let Some(signature) = &self.signature {
            let is_known = signature.starts_with("sha256:") || signature.starts_with("ed25519:");
            if !is_known {
                anyhow::bail!(
                    "Plugin '{}' signature must start with sha256: or ed25519:",
                    self.id
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_manifest() -> PluginManifest {
        PluginManifest {
            id: "plugin-x".to_string(),
            version: "1.2.3".to_string(),
            name: "Plugin X".to_string(),
            description: "demo".to_string(),
            capabilities: vec!["search".to_string()],
            permissions: vec![],
            risk_declaration: RiskDeclaration::default(),
            distribution_channel: "stable".to_string(),
            min_runtime_version: Some("1.0.0".to_string()),
            signature: None,
            transport: PluginTransport {
                r#type: "stdio".to_string(),
                command: Some("plugin-x".to_string()),
                args: vec![],
                url: None,
            },
        }
    }

    #[test]
    fn test_manifest_validation_accepts_compatible_runtime() {
        let manifest = base_manifest();
        manifest.validate_for_runtime("1.0.5").expect("compatible");
    }

    #[test]
    fn test_manifest_validation_rejects_invalid_semver() {
        let mut manifest = base_manifest();
        manifest.version = "not-semver".to_string();
        assert!(manifest.validate_for_runtime("1.0.0").is_err());
    }

    #[test]
    fn test_manifest_validation_rejects_incompatible_runtime() {
        let manifest = base_manifest();
        assert!(manifest.validate_for_runtime("0.9.0").is_err());
    }

    #[test]
    fn test_manifest_validation_rejects_unknown_distribution_channel() {
        let mut manifest = base_manifest();
        manifest.distribution_channel = "beta".to_string();
        assert!(manifest.validate_for_runtime("1.0.0").is_err());
    }
}
