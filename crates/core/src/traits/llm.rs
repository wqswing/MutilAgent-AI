//! L-M Model Gateway traits.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::error::Result;

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
