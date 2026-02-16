use super::refs::RefId;
use serde::{Deserialize, Serialize};

// =============================================================================
// Tool Types (L2)
// =============================================================================

/// Output from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Whether the tool execution was successful.
    pub success: bool,

    /// Output content (may be a RefId for large outputs).
    pub content: String,

    /// Optional structured data.
    pub data: Option<serde_json::Value>,

    /// References created during execution.
    pub created_refs: Vec<RefId>,
}

impl ToolOutput {
    /// Create a successful text output.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: content.into(),
            data: None,
            created_refs: Vec::new(),
        }
    }

    /// Create a successful output with structured data.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Create a reference output (for large content).
    pub fn reference(ref_id: RefId, summary: impl Into<String>) -> Self {
        Self {
            success: true,
            content: format!("Output saved as RefID: {}. {}", ref_id, summary.into()),
            data: None,
            created_refs: vec![ref_id],
        }
    }

    /// Create a failed output.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            content: message.into(),
            data: None,
            created_refs: Vec::new(),
        }
    }
}

/// Tool definition for the tool registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique tool name.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// JSON Schema for tool arguments.
    pub parameters: serde_json::Value,

    /// Whether the tool supports streaming output.
    pub supports_streaming: bool,
}
