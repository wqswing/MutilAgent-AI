//! Tool registry implementation.

use async_trait::async_trait;
use dashmap::DashMap;

use std::sync::Arc;
use multi_agent_core::{
    traits::{Tool, ToolRegistry},
    types::{ToolDefinition, ToolOutput},
    Error, Result,
};

/// Thread-safe wrapper for tools.
struct ToolEntry {
    tool: Arc<dyn Tool>,
}

// Safety: Tool trait requires Send + Sync
unsafe impl Send for ToolEntry {}
unsafe impl Sync for ToolEntry {}

/// Default tool registry using DashMap.
pub struct DefaultToolRegistry {
    /// Registered tools.
    tools: DashMap<String, ToolEntry>,
}

impl DefaultToolRegistry {
    /// Create a new tool registry.
    pub fn new() -> Self {
        Self {
            tools: DashMap::new(),
        }
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for DefaultToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolRegistry for DefaultToolRegistry {
    async fn register(&self, tool: Box<dyn Tool>) -> Result<()> {
        let name = tool.name().to_string();
        tracing::info!(tool = %name, "Registering tool");

        if self.tools.contains_key(&name) {
            return Err(Error::Internal(format!(
                "Tool '{}' is already registered",
                name
            )));
        }

        self.tools.insert(name, ToolEntry { tool: Arc::from(tool) });
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<Box<dyn Tool>>> {
        if let Some(entry) = self.tools.get(name) {
            // Return a wrapper that holds the Arc
            let wrapper = LocalToolWrapper { tool: entry.tool.clone() };
            return Ok(Some(Box::new(wrapper)));
        }
        Ok(None)
    }

    async fn list(&self) -> Result<Vec<ToolDefinition>> {
        let definitions: Vec<_> = self
            .tools
            .iter()
            .map(|entry| ToolDefinition {
                name: entry.tool.name().to_string(),
                description: entry.tool.description().to_string(),
                parameters: entry.tool.parameters(),
                supports_streaming: false,
            })
            .collect();

        Ok(definitions)
    }

    async fn execute(&self, name: &str, args: serde_json::Value) -> Result<ToolOutput> {
        let entry = self
            .tools
            .get(name)
            .ok_or_else(|| Error::tool_not_found(name))?;

        tracing::debug!(tool = %name, "Executing tool");

        entry.tool.execute(args).await
    }
}

/// Wrapper for Arc<dyn Tool> to allow returning Box<dyn Tool>
struct LocalToolWrapper {
    tool: Arc<dyn Tool>,
}

#[async_trait]
impl Tool for LocalToolWrapper {
    fn name(&self) -> &str {
        self.tool.name()
    }
    
    fn description(&self) -> &str {
        self.tool.description()
    }
    
    fn parameters(&self) -> serde_json::Value {
        self.tool.parameters()
    }
    
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput> {
        self.tool.execute(args).await
    }
}

/// Create a registry with built-in tools.
pub fn create_default_registry() -> DefaultToolRegistry {
    let registry = DefaultToolRegistry::new();

    // Register built-in tools
    // Tools will be registered in the main function

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::EchoTool;

    #[tokio::test]
    async fn test_register_and_list() {
        let registry = DefaultToolRegistry::new();

        registry.register(Box::new(EchoTool)).await.unwrap();

        let tools = registry.list().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
    }

    #[tokio::test]
    async fn test_execute() {
        let registry = DefaultToolRegistry::new();

        registry.register(Box::new(EchoTool)).await.unwrap();

        let result = registry
            .execute("echo", serde_json::json!({"message": "Hello"}))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.content.contains("Hello"));
    }

    #[tokio::test]
    async fn test_execute_not_found() {
        let registry = DefaultToolRegistry::new();

        let result = registry.execute("nonexistent", serde_json::json!({})).await;

        assert!(result.is_err());
    }
}
