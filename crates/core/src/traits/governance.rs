//! L4 Governance traits.

use async_trait::async_trait;
use serde_json::Value;
use crate::error::Result;
use crate::types::{AgentResult, NormalizedRequest};

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
