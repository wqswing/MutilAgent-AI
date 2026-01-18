use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use multi_agent_core::{Result, Error};
use multi_agent_core::traits::{Tool, ToolRegistry};
use multi_agent_core::types::{ToolDefinition, ToolOutput};

/// A registry that aggregates multiple other registries.
pub struct CompositeToolRegistry {
    registries: Vec<Arc<dyn ToolRegistry>>,
}

impl CompositeToolRegistry {
    /// Create a new empty composite registry.
    pub fn new() -> Self {
        Self {
            registries: Vec::new(),
        }
    }

    /// Add a registry to the composite.
    pub fn add_registry(&mut self, registry: Arc<dyn ToolRegistry>) {
        self.registries.push(registry);
    }
}

#[async_trait]
impl ToolRegistry for CompositeToolRegistry {
    async fn register(&self, _tool: Box<dyn Tool>) -> Result<()> {
        Err(Error::internal("Cannot register tools directly to CompositeToolRegistry. Register to a child registry instead."))
    }

    async fn get(&self, name: &str) -> Result<Option<Box<dyn Tool>>> {
        for registry in &self.registries {
            if let Ok(Some(tool)) = registry.get(name).await {
                return Ok(Some(tool));
            }
        }
        Ok(None)
    }

    async fn list(&self) -> Result<Vec<ToolDefinition>> {
        let mut all_tools = Vec::new();
        for registry in &self.registries {
            if let Ok(tools) = registry.list().await {
                all_tools.extend(tools);
            }
        }
        Ok(all_tools)
    }

    async fn execute(&self, name: &str, args: Value) -> Result<ToolOutput> {
        for registry in &self.registries {
            // Check if registry has the tool first to avoid blind execution attempts if possible,
            // or just try execute and catch specific "not found" errors?
            // Most registries might error if tool not found. 
            // Better to check `get` or rely on `list`.
            // However, `get` returns a Tool, which we can then use?
            // But `ToolRegistry::execute` is the standard way.
            
            // Optimization: Try to find which registry has it.
            if let Ok(Some(_)) = registry.get(name).await {
                return registry.execute(name, args).await;
            }
        }
        Err(Error::tool_not_found(name))
    }
}
