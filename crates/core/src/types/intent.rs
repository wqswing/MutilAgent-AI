use serde::{Deserialize, Serialize};

// =============================================================================
// Intent Types (L0 Router Output)
// =============================================================================

/// User intent classification result from L0.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum UserIntent {
    /// Fast path: direct tool invocation without L1 overhead.
    #[serde(rename = "fast_action")]
    FastAction {
        /// Name of the tool to invoke.
        tool_name: String,
        /// Arguments for the tool.
        args: serde_json::Value,
        /// User ID for isolation.
        #[serde(default)]
        user_id: Option<String>,
    },

    /// Slow path: start L1 Controller for complex reasoning.
    #[serde(rename = "complex_mission")]
    ComplexMission {
        /// High-level goal extracted from the request.
        goal: String,
        /// Summarized context from L0 preprocessing.
        context_summary: String,
        /// Visual references (image RefIds).
        visual_refs: Vec<String>,
        /// User ID for isolation.
        #[serde(default)]
        user_id: Option<String>,
    },
}
