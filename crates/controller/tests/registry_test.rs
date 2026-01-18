use std::sync::Arc;
use multi_agent_core::traits::{Tool, ToolRegistry};
use multi_agent_core::types::ToolOutput;
use multi_agent_controller::react::ReActController;
use multi_agent_skills::{CompositeToolRegistry, DefaultToolRegistry, McpRegistry};
use serde_json::json;

// Mock Tool
struct MockLocalTool;
#[async_trait::async_trait]
impl Tool for MockLocalTool {
    fn name(&self) -> &str { "local_tool" }
    fn description(&self) -> &str { "A local tool" }
    fn parameters(&self) -> serde_json::Value { json!({}) }
    async fn execute(&self, _args: serde_json::Value) -> multi_agent_core::Result<ToolOutput> {
        Ok(ToolOutput::text("Local tool executed".to_string()))
    }
}

#[tokio::test]
async fn test_unified_registry_routing() {
    // 1. Setup Composite Registry
    let mut composite = CompositeToolRegistry::new();
    
    // 2. Add Local Registry
    let local_registry = DefaultToolRegistry::new();
    local_registry.register(Box::new(MockLocalTool)).await.unwrap();
    composite.add_registry(Arc::new(local_registry));

    // 3. Add MCP Registry (Mocked)
    let mcp_registry = Arc::new(McpRegistry::new());
    composite.add_registry(mcp_registry.clone());
    
    let composite_arc = Arc::new(composite);

    // 4. Verify routing finding local tool
    let found = composite_arc.get("local_tool").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name(), "local_tool");

    // 5. Verify Controller Integration
    let controller = ReActController::builder()
        .with_tools(composite_arc.clone())
        .build();
        
    // (Controller logic for execution uses self.tools.execute(), which maps to Composite::execute)
    
    let output = composite_arc.execute("local_tool", json!({})).await;
    assert!(output.is_ok());
    assert_eq!(output.unwrap().content, "Local tool executed");
}
