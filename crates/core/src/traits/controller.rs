//! L1 Controller traits.

use async_trait::async_trait;
use serde_json::Value;
use crate::error::Result;
use crate::types::AgentResult;

/// Controller for orchestrating complex tasks.
#[async_trait]
pub trait Controller: Send + Sync {
    /// Execute a complex mission through the ReAct loop.
    async fn execute(&self, intent: crate::types::UserIntent) -> Result<AgentResult>;

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
