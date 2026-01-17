//! Core traits for MutilAgent.
//!
//! These traits define the contracts that components must implement
//! across the different layers of the system.

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;

use crate::error::Result;
use crate::types::{
    AgentResult, NormalizedRequest, RefId, ToolDefinition, ToolOutput, UserIntent,
};

// =============================================================================
// L0 Gateway Traits
// =============================================================================

/// Intent router for classifying incoming requests.
#[async_trait]
pub trait IntentRouter: Send + Sync {
    /// Classify the intent of a normalized request.
    async fn classify(&self, request: &NormalizedRequest) -> Result<UserIntent>;
}

/// Semantic cache for high-frequency queries.
#[async_trait]
pub trait SemanticCache: Send + Sync {
    /// Check if a similar query exists in the cache.
    /// Returns the cached response if similarity > threshold.
    async fn get(&self, query: &str) -> Result<Option<String>>;

    /// Store a query-response pair in the cache.
    async fn set(&self, query: &str, response: &str) -> Result<()>;

    /// Invalidate cache entries matching a pattern.
    async fn invalidate(&self, pattern: &str) -> Result<()>;
}

// =============================================================================
// L1 Controller Traits
// =============================================================================

/// Controller for orchestrating complex tasks.
#[async_trait]
pub trait Controller: Send + Sync {
    /// Execute a complex mission through the ReAct loop.
    async fn execute(&self, intent: UserIntent) -> Result<AgentResult>;

    /// Resume a previously interrupted task.
    async fn resume(&self, session_id: &str) -> Result<AgentResult>;

    /// Cancel a running task.
    async fn cancel(&self, session_id: &str) -> Result<()>;
}

/// SOP (Standard Operating Procedure) engine.
#[async_trait]
pub trait SopEngine: Send + Sync {
    /// Load an SOP definition from YAML.
    async fn load(&self, yaml: &str) -> Result<SopDefinition>;

    /// Execute an SOP with the given context.
    async fn execute(&self, sop: &SopDefinition, context: Value) -> Result<AgentResult>;
}

/// Session store for persistence.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Save a session.
    async fn save(&self, session: &crate::types::Session) -> Result<()>;
    
    /// Load a session by ID.
    async fn load(&self, session_id: &str) -> Result<Option<crate::types::Session>>;
    
    /// Delete a session.
    async fn delete(&self, session_id: &str) -> Result<()>;
    
    /// List all running sessions.
    async fn list_running(&self) -> Result<Vec<String>>;
}

/// SOP definition structure.
#[derive(Debug, Clone)]
pub struct SopDefinition {
    /// SOP name.
    pub name: String,
    /// SOP steps.
    pub steps: Vec<SopStep>,
    /// Whether steps can be parallelized.
    pub allow_parallel: bool,
}

/// A single step in an SOP.
#[derive(Debug, Clone)]
pub struct SopStep {
    /// Step name.
    pub name: String,
    /// Tool to execute.
    pub tool: String,
    /// Arguments for the tool.
    pub args: Value,
    /// Dependencies on other steps.
    pub depends_on: Vec<String>,
    /// Tools allowed for this step (privilege de-escalation).
    /// If empty, all tools are allowed.
    pub allow_tools: Vec<String>,
}

// =============================================================================
// L2 Skills Traits
// =============================================================================

/// Tool interface for atomic operations.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the unique name of the tool.
    fn name(&self) -> &str;

    /// Get the human-readable description.
    fn description(&self) -> &str;

    /// Get the JSON Schema for parameters.
    fn parameters(&self) -> Value;

    /// Execute the tool with the given arguments.
    async fn execute(&self, args: Value) -> Result<ToolOutput>;
}

/// Tool registry for managing available tools.
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    /// Register a new tool.
    async fn register(&self, tool: Box<dyn Tool>) -> Result<()>;

    /// Get a tool by name.
    async fn get(&self, name: &str) -> Result<Option<Box<dyn Tool>>>;

    /// List all available tools.
    async fn list(&self) -> Result<Vec<ToolDefinition>>;

    /// Execute a tool by name with arguments.
    async fn execute(&self, name: &str, args: Value) -> Result<ToolOutput>;
}

/// MCP (Model Context Protocol) adapter.
#[async_trait]
pub trait McpAdapter: Send + Sync {
    /// Connect to an MCP server.
    async fn connect(&mut self, server_url: &str) -> Result<()>;

    /// Disconnect from the MCP server.
    async fn disconnect(&mut self) -> Result<()>;

    /// List available tools from the MCP server.
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>>;

    /// Execute a tool on the MCP server.
    async fn execute_tool(&self, name: &str, args: Value) -> Result<ToolOutput>;
}

// =============================================================================
// L3 Artifact Store Traits
// =============================================================================

/// Artifact store for managing large content.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// Save data and return a reference ID.
    async fn save(&self, data: Bytes) -> Result<RefId>;

    /// Save data with a specific content type.
    async fn save_with_type(&self, data: Bytes, content_type: &str) -> Result<RefId>;

    /// Load data by reference ID.
    async fn load(&self, id: &RefId) -> Result<Option<Bytes>>;

    /// Delete an artifact.
    async fn delete(&self, id: &RefId) -> Result<()>;

    /// Check if an artifact exists.
    async fn exists(&self, id: &RefId) -> Result<bool>;

    /// Get metadata about an artifact.
    async fn metadata(&self, id: &RefId) -> Result<Option<ArtifactMetadata>>;
}

/// Metadata for stored artifacts.
#[derive(Debug, Clone)]
pub struct ArtifactMetadata {
    /// Size in bytes.
    pub size: usize,
    /// Content type.
    pub content_type: String,
    /// Creation timestamp.
    pub created_at: i64,
    /// Storage tier.
    pub tier: StorageTier,
}

/// Storage tier for tiered storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTier {
    /// Hot storage (in-memory, fastest).
    Hot,
    /// Warm storage (Redis, fast).
    Warm,
    /// Cold storage (S3, cheapest).
    Cold,
}

// =============================================================================
// L4 Governance Traits
// =============================================================================

/// Budget controller for token management.
#[async_trait]
pub trait BudgetController: Send + Sync {
    /// Reserve tokens from the budget.
    async fn reserve(&self, session_id: &str, tokens: u64) -> Result<()>;

    /// Release reserved tokens.
    async fn release(&self, session_id: &str, tokens: u64) -> Result<()>;

    /// Record actual token usage.
    async fn record_usage(&self, session_id: &str, prompt: u64, completion: u64) -> Result<()>;

    /// Check remaining budget.
    async fn remaining(&self, session_id: &str) -> Result<u64>;

    /// Check if budget is exceeded.
    async fn is_exceeded(&self, session_id: &str) -> Result<bool>;
}

/// Security proxy for request validation.
#[async_trait]
pub trait SecurityProxy: Send + Sync {
    /// Validate an incoming request.
    async fn validate_request(&self, request: &NormalizedRequest) -> Result<()>;

    /// Validate tool arguments.
    async fn validate_tool_args(&self, tool: &str, args: &Value) -> Result<()>;

    /// Validate output before returning to user.
    async fn validate_output(&self, output: &AgentResult) -> Result<()>;
}

// =============================================================================
// L-M Model Gateway Traits
// =============================================================================

/// LLM client interface.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Generate a completion.
    async fn complete(&self, prompt: &str) -> Result<LlmResponse>;

    /// Generate a chat completion.
    async fn chat(&self, messages: &[ChatMessage]) -> Result<LlmResponse>;

    /// Generate embeddings for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// Chat message for LLM interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role (system, user, assistant, tool).
    pub role: String,
    /// Message content.
    pub content: String,
    /// Optional tool calls.
    pub tool_calls: Option<Vec<Value>>,
}

use serde::{Deserialize, Serialize};

/// Response from an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// Generated content.
    pub content: String,
    /// Finish reason.
    pub finish_reason: String,
    /// Token usage.
    pub usage: LlmUsage,
    /// Optional tool calls.
    pub tool_calls: Option<Vec<Value>>,
}

/// Token usage from LLM call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmUsage {
    /// Prompt tokens.
    pub prompt_tokens: u64,
    /// Completion tokens.
    pub completion_tokens: u64,
    /// Total tokens.
    pub total_tokens: u64,
}

/// Model selector for load balancing.
#[async_trait]
pub trait ModelSelector: Send + Sync {
    /// Select the best available model for a tier.
    async fn select(&self, tier: crate::types::ModelTier) -> Result<Box<dyn LlmClient>>;

    /// Report a model failure.
    async fn report_failure(&self, provider: &str, model: &str) -> Result<()>;

    /// Report a model success.
    async fn report_success(&self, provider: &str, model: &str) -> Result<()>;
}
