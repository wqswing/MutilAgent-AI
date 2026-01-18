//! Error types for Multiagent.

use thiserror::Error;

/// Result type alias using Multiagent's Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Core error type for Multiagent.
#[derive(Error, Debug)]
pub enum Error {
    // =========================================================================
    // Gateway Errors (L0)
    // =========================================================================
    #[error("Gateway error: {0}")]
    Gateway(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Routing failed: {0}")]
    RoutingFailed(String),

    #[error("Semantic cache error: {0}")]
    SemanticCache(String),

    // =========================================================================
    // Controller Errors (L1)
    // =========================================================================
    #[error("Controller error: {0}")]
    Controller(String),

    #[error("ReAct loop exceeded max iterations: {0}")]
    MaxIterationsExceeded(usize),

    #[error("State persistence error: {0}")]
    StatePersistence(String),

    #[error("SOP execution error: {0}")]
    SopExecution(String),

    // =========================================================================
    // Skills Errors (L2)
    // =========================================================================
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool execution failed: {0}")]
    ToolExecution(String),

    #[error("MCP adapter error: {0}")]
    McpAdapter(String),

    // =========================================================================
    // Store Errors (L3)
    // =========================================================================
    #[error("Artifact not found: {0}")]
    ArtifactNotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    // =========================================================================
    // Governance Errors (L4)
    // =========================================================================
    #[error("Budget exceeded: used {used}, limit {limit}")]
    BudgetExceeded { used: u64, limit: u64 },

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Governance error: {0}")]
    Governance(String),

    // =========================================================================
    // Model Gateway Errors (L-M)
    // =========================================================================
    #[error("Model provider error: {0}")]
    ModelProvider(String),

    #[error("All providers unavailable")]
    AllProvidersUnavailable,

    #[error("Model selection failed: {0}")]
    ModelSelection(String),

    // =========================================================================
    // Template Errors (L-T)
    // =========================================================================
    #[error("Template hydration error: {0}")]
    Template(String),

    // =========================================================================
    // Generic Errors
    // =========================================================================
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),


}

impl Error {
    /// Create a gateway error.
    pub fn gateway(msg: impl Into<String>) -> Self {
        Self::Gateway(msg.into())
    }

    /// Create an invalid request error.
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::InvalidRequest(msg.into())
    }

    /// Create a controller error.
    pub fn controller(msg: impl Into<String>) -> Self {
        Self::Controller(msg.into())
    }

    /// Create a tool not found error.
    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::ToolNotFound(name.into())
    }

    /// Create a tool execution error.
    pub fn tool_execution(msg: impl Into<String>) -> Self {
        Self::ToolExecution(msg.into())
    }

    /// Create an MCP adapter error.
    pub fn mcp_adapter(msg: impl Into<String>) -> Self {
        Self::McpAdapter(msg.into())
    }

    /// Create a storage error.
    pub fn storage(msg: impl Into<String>) -> Self {
        Self::Storage(msg.into())
    }

    /// Create a governance error.
    pub fn governance(msg: impl Into<String>) -> Self {
        Self::Governance(msg.into())
    }

    /// Create an internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

}
