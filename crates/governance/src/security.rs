//! Security proxy for request validation.

use async_trait::async_trait;
use std::collections::HashSet;

use multi_agent_core::{
    traits::SecurityProxy,
    types::{AgentResult, NormalizedRequest},
    Error, Result,
};

/// Configuration for the security proxy.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Maximum request content length.
    pub max_content_length: usize,
    /// Blocked tool names.
    pub blocked_tools: HashSet<String>,
    /// Blocked patterns in content.
    pub blocked_patterns: Vec<String>,
    /// Enable output validation.
    pub validate_output: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_content_length: 100_000, // 100KB
            blocked_tools: HashSet::new(),
            blocked_patterns: vec![
                // Example patterns to block
                "rm -rf".to_string(),
                "sudo".to_string(),
                "password".to_string(),
            ],
            validate_output: true,
        }
    }
}

/// Default security proxy implementation.
pub struct DefaultSecurityProxy {
    config: SecurityConfig,
}

impl DefaultSecurityProxy {
    /// Create a new security proxy with default config.
    pub fn new() -> Self {
        Self {
            config: SecurityConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: SecurityConfig) -> Self {
        Self { config }
    }

    /// Add a blocked tool.
    pub fn block_tool(mut self, tool: impl Into<String>) -> Self {
        self.config.blocked_tools.insert(tool.into());
        self
    }

    /// Add a blocked pattern.
    pub fn block_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.config.blocked_patterns.push(pattern.into());
        self
    }

    /// Check content for blocked patterns.
    fn check_patterns(&self, content: &str) -> Result<()> {
        let lower = content.to_lowercase();
        for pattern in &self.config.blocked_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                tracing::warn!(
                    pattern = pattern,
                    "Blocked pattern detected"
                );
                return Err(Error::SecurityViolation(format!(
                    "Request contains blocked pattern: {}",
                    pattern
                )));
            }
        }
        Ok(())
    }
}

impl Default for DefaultSecurityProxy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecurityProxy for DefaultSecurityProxy {
    async fn validate_request(&self, request: &NormalizedRequest) -> Result<()> {
        // Check content length
        if request.content.len() > self.config.max_content_length {
            return Err(Error::SecurityViolation(format!(
                "Request content too large: {} > {}",
                request.content.len(),
                self.config.max_content_length
            )));
        }

        // Check for blocked patterns
        self.check_patterns(&request.content)?;

        tracing::debug!(
            trace_id = %request.trace_id,
            "Request validated"
        );

        Ok(())
    }

    async fn validate_tool_args(&self, tool: &str, args: &serde_json::Value) -> Result<()> {
        // Check if tool is blocked
        if self.config.blocked_tools.contains(tool) {
            return Err(Error::SecurityViolation(format!(
                "Tool '{}' is blocked",
                tool
            )));
        }

        // Check args for blocked patterns
        let args_str = serde_json::to_string(args).unwrap_or_default();
        self.check_patterns(&args_str)?;

        tracing::debug!(tool = tool, "Tool args validated");

        Ok(())
    }

    async fn validate_output(&self, output: &AgentResult) -> Result<()> {
        if !self.config.validate_output {
            return Ok(());
        }

        // Check output for sensitive data patterns
        match output {
            AgentResult::Text(text) => {
                // Could add PII detection here
                if text.len() > 1_000_000 {
                    return Err(Error::SecurityViolation(
                        "Output too large".to_string(),
                    ));
                }
            }
            AgentResult::Error { message, .. } => {
                // Ensure errors don't leak sensitive info
                // In production, sanitize error messages
                tracing::debug!(message = message, "Error output validated");
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use multi_agent_core::types::RequestMetadata; // Removed unused

    #[tokio::test]
    async fn test_validate_request() {
        let proxy = DefaultSecurityProxy::new();

        let request = NormalizedRequest::text("Hello, world!");
        assert!(proxy.validate_request(&request).await.is_ok());
    }

    #[tokio::test]
    async fn test_blocked_pattern() {
        let proxy = DefaultSecurityProxy::new();

        let request = NormalizedRequest::text("Please run rm -rf /");
        let result = proxy.validate_request(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_content_too_large() {
        let proxy = DefaultSecurityProxy::with_config(SecurityConfig {
            max_content_length: 10,
            ..Default::default()
        });

        let request = NormalizedRequest::text("This is too long for the limit");
        let result = proxy.validate_request(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_blocked_tool() {
        let proxy = DefaultSecurityProxy::new().block_tool("dangerous_tool");

        let result = proxy
            .validate_tool_args("dangerous_tool", &serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }
}
