//! Built-in tools.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use multi_agent_core::{
    traits::{ArtifactStore, Tool},
    types::{RefId, ToolOutput},
    Result,
};

// =============================================================================
// Echo Tool
// =============================================================================

/// Simple echo tool for testing.
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes the input message back"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to echo"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("No message provided");

        Ok(ToolOutput::text(format!("Echo: {}", message)))
    }
}

// =============================================================================
// Read Artifact Tool
// =============================================================================

/// Tool for reading artifacts from L3 store.
pub struct ReadArtifactTool {
    store: Arc<dyn ArtifactStore>,
}

impl ReadArtifactTool {
    /// Create a new read artifact tool.
    pub fn new(store: Arc<dyn ArtifactStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for ReadArtifactTool {
    fn name(&self) -> &str {
        "read_artifact"
    }

    fn description(&self) -> &str {
        "Read content from a stored artifact using its RefID"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ref_id": {
                    "type": "string",
                    "description": "The reference ID of the artifact to read"
                }
            },
            "required": ["ref_id"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let ref_id_str = args
            .get("ref_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| multi_agent_core::Error::invalid_request("ref_id is required"))?;

        let ref_id = RefId::from_string(ref_id_str);

        match self.store.load(&ref_id).await? {
            Some(bytes) => {
                let content = String::from_utf8_lossy(&bytes).to_string();
                Ok(ToolOutput::text(content))
            }
            None => Ok(ToolOutput::error(format!(
                "Artifact not found: {}",
                ref_id_str
            ))),
        }
    }
}

// =============================================================================
// Calculator Tool
// =============================================================================

/// Simple calculator tool.
pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Perform basic arithmetic operations"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["add", "subtract", "multiply", "divide"],
                    "description": "The arithmetic operation to perform"
                },
                "a": {
                    "type": "number",
                    "description": "First operand"
                },
                "b": {
                    "type": "number",
                    "description": "Second operand"
                }
            },
            "required": ["operation", "a", "b"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let operation = args
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| multi_agent_core::Error::invalid_request("operation is required"))?;

        let a = args
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| multi_agent_core::Error::invalid_request("a must be a number"))?;

        let b = args
            .get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| multi_agent_core::Error::invalid_request("b must be a number"))?;

        let result = match operation {
            "add" => a + b,
            "subtract" => a - b,
            "multiply" => a * b,
            "divide" => {
                if b == 0.0 {
                    return Ok(ToolOutput::error("Division by zero"));
                }
                a / b
            }
            _ => {
                return Ok(ToolOutput::error(format!(
                    "Unknown operation: {}",
                    operation
                )));
            }
        };

        Ok(ToolOutput::text(format!("{} {} {} = {}", a, operation, b, result))
            .with_data(json!({ "result": result })))
    }
}

// =============================================================================
// List Tools Tool
// =============================================================================

/// Tool for listing all available tools.
pub struct ListToolsTool {
    registry: Arc<dyn multi_agent_core::traits::ToolRegistry>,
}

impl ListToolsTool {
    /// Create a new list tools tool.
    pub fn new(registry: Arc<dyn multi_agent_core::traits::ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ListToolsTool {
    fn name(&self) -> &str {
        "list_tools"
    }

    fn description(&self) -> &str {
        "List all available tools and their descriptions"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: Value) -> Result<ToolOutput> {
        let tools = self.registry.list().await?;

        let mut output = String::from("Available tools:\n\n");
        for tool in &tools {
            output.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
        }

        Ok(ToolOutput::text(output).with_data(json!({ "tools": tools })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_echo_tool() {
        let tool = EchoTool;

        let result = tool
            .execute(json!({"message": "Hello, World!"}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.content, "Echo: Hello, World!");
    }

    #[tokio::test]
    async fn test_calculator_add() {
        let tool = CalculatorTool;

        let result = tool
            .execute(json!({"operation": "add", "a": 5, "b": 3}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.data.unwrap()["result"], 8.0);
    }

    #[tokio::test]
    async fn test_calculator_divide_by_zero() {
        let tool = CalculatorTool;

        let result = tool
            .execute(json!({"operation": "divide", "a": 5, "b": 0}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.content.contains("Division by zero"));
    }
}
