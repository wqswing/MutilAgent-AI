use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =============================================================================
// Session & State Types
// =============================================================================

/// Session state for persistent conversations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID.
    pub id: String,

    /// Trace ID for the session/task.
    pub trace_id: String,

    /// User ID of the session owner (for isolation).
    pub user_id: Option<String>,

    /// Current status.
    pub status: SessionStatus,

    /// Conversation history.
    pub history: Vec<HistoryEntry>,

    /// Current task state (for resurrection).
    pub task_state: Option<TaskState>,

    /// Token usage tracking.
    pub token_usage: TokenUsage,

    /// Creation timestamp.
    pub created_at: i64,

    /// Last updated timestamp.
    pub updated_at: i64,
}

/// Session status for state tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session is actively processing.
    Running,
    /// Session is paused/waiting.
    Paused,
    /// Session completed successfully.
    Completed,
    /// Session failed with error.
    Failed,
}

/// Entry in conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Role (user, assistant, system, tool).
    pub role: String,

    /// Content of the message.
    pub content: Arc<String>,

    /// Optional tool call information.
    pub tool_call: Option<ToolCallInfo>,

    /// Timestamp.
    pub timestamp: i64,
}

/// Information about a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// Tool name.
    pub name: String,
    /// Tool arguments.
    pub arguments: serde_json::Value,
    /// Tool result (if completed).
    pub result: Option<Arc<String>>,
}

/// Task state for resurrection pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    /// Current ReAct loop iteration.
    pub iteration: usize,

    /// Current goal.
    pub goal: String,

    /// Accumulated observations.
    pub observations: Vec<Arc<String>>,

    /// Pending actions.
    pub pending_actions: Vec<serde_json::Value>,

    /// Consecutive HITL rejections (for deadlock circuit breaker).
    #[serde(default)]
    pub consecutive_rejections: usize,
}

/// Token usage tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens used for prompts.
    pub prompt_tokens: u64,

    /// Tokens used for completions.
    pub completion_tokens: u64,

    /// Total tokens used.
    pub total_tokens: u64,

    /// Budget limit.
    pub budget_limit: u64,
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            budget_limit: 1_000_000,
        }
    }
}

impl TokenUsage {
    /// Create new token usage with a budget limit.
    pub fn with_budget(limit: u64) -> Self {
        Self {
            budget_limit: limit,
            ..Default::default()
        }
    }

    /// Add usage to the tracker.
    pub fn add(&mut self, prompt: u64, completion: u64) {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.total_tokens += prompt + completion;
    }

    /// Check if budget is exceeded.
    pub fn is_exceeded(&self) -> bool {
        self.total_tokens >= self.budget_limit
    }

    /// Get remaining budget.
    pub fn remaining(&self) -> u64 {
        self.budget_limit.saturating_sub(self.total_tokens)
    }
}
