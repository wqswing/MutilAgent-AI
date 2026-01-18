//! Hierarchical Subagent Delegation System.
//! 
//! Enables parent agents to spawn child agents with specific objectives
//! and isolated contexts for divide-and-conquer problem solving.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use multi_agent_core::{Result, traits::{LlmClient, ChatMessage}};

/// A delegation request from parent to child agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationRequest {
    /// Unique ID for this delegation.
    pub id: String,
    /// Objective for the child agent.
    pub objective: String,
    /// Context to pass to the child.
    pub context: String,
    /// Maximum iterations for the child.
    pub max_iterations: usize,
    /// Tools the child is allowed to use.
    pub allowed_tools: Vec<String>,
}

impl DelegationRequest {
    /// Create a new delegation request.
    pub fn new(objective: impl Into<String>) -> Self {
        Self {
            id: format!("del_{}", Uuid::new_v4().to_string().split('-').next().unwrap()),
            objective: objective.into(),
            context: String::new(),
            max_iterations: 10,
            allowed_tools: Vec::new(),
        }
    }
    
    /// Add context for the child agent.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = context.into();
        self
    }
    
    /// Set maximum iterations.
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
    
    /// Set allowed tools.
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }
}

/// Result from a delegated subagent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResult {
    /// ID of the delegation request.
    pub delegation_id: String,
    /// Whether the child succeeded.
    pub success: bool,
    /// Result content from the child.
    pub result: String,
    /// Number of iterations used.
    pub iterations_used: usize,
    /// Any error message.
    pub error: Option<String>,
}

impl DelegationResult {
    /// Create a success result.
    pub fn success(delegation_id: String, result: String, iterations: usize) -> Self {
        Self {
            delegation_id,
            success: true,
            result,
            iterations_used: iterations,
            error: None,
        }
    }
    
    /// Create a failure result.
    pub fn failure(delegation_id: String, error: String) -> Self {
        Self {
            delegation_id,
            success: false,
            result: String::new(),
            iterations_used: 0,
            error: Some(error),
        }
    }
}

/// Subagent executor that runs delegated tasks in isolated contexts.
pub struct SubAgentExecutor<C: LlmClient> {
    client: C,
}

impl<C: LlmClient> SubAgentExecutor<C> {
    /// Create a new subagent executor.
    pub fn new(client: C) -> Self {
        Self { client }
    }
    
    /// Execute a delegated task.
    pub async fn execute(&self, request: DelegationRequest) -> Result<DelegationResult> {
        tracing::info!(id = %request.id, objective = %request.objective, "Starting subagent execution");
        
        // Build isolated context for child agent
        let system_prompt = format!(
            "You are a focused subagent with a specific objective.\n\
             Objective: {}\n\
             Context: {}\n\
             You must complete this objective concisely and return your result.",
            request.objective,
            request.context
        );
        
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!("Complete the objective: {}", request.objective),
                tool_calls: None,
            },
        ];
        
        // Execute with isolated context
        match self.client.chat(&messages).await {
            Ok(response) => {
                tracing::info!(id = %request.id, "Subagent completed successfully");
                Ok(DelegationResult::success(
                    request.id,
                    response.content,
                    1,
                ))
            }
            Err(e) => {
                tracing::error!(id = %request.id, error = %e, "Subagent failed");
                Ok(DelegationResult::failure(request.id, e.to_string()))
            }
        }
    }
}

/// Trait for components that can delegate work to subagents.
#[async_trait]
pub trait Delegator: Send + Sync {
    /// Delegate a task to a subagent.
    async fn delegate(&self, request: DelegationRequest) -> Result<DelegationResult>;
    
    /// Check if a delegation is complete.
    async fn check_delegation(&self, id: &str) -> Result<Option<DelegationResult>>;
}

/// In-memory delegation manager for tracking subagent tasks.
pub struct DelegationManager<C: LlmClient> {
    executor: SubAgentExecutor<C>,
    results: std::sync::Arc<dashmap::DashMap<String, DelegationResult>>,
}

impl<C: LlmClient> DelegationManager<C> {
    /// Create a new delegation manager.
    pub fn new(client: C) -> Self {
        Self {
            executor: SubAgentExecutor::new(client),
            results: std::sync::Arc::new(dashmap::DashMap::new()),
        }
    }
}

#[async_trait]
impl<C: LlmClient + 'static> Delegator for DelegationManager<C> {
    async fn delegate(&self, request: DelegationRequest) -> Result<DelegationResult> {
        let id = request.id.clone();
        let result = self.executor.execute(request).await?;
        self.results.insert(id, result.clone());
        Ok(result)
    }
    
    async fn check_delegation(&self, id: &str) -> Result<Option<DelegationResult>> {
        Ok(self.results.get(id).map(|r| r.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_delegation_request_builder() {
        let request = DelegationRequest::new("Summarize the document")
            .with_context("Document is about AI safety")
            .with_max_iterations(5)
            .with_tools(vec!["read_file".to_string()]);
        
        assert!(request.id.starts_with("del_"));
        assert_eq!(request.objective, "Summarize the document");
        assert_eq!(request.context, "Document is about AI safety");
        assert_eq!(request.max_iterations, 5);
        assert_eq!(request.allowed_tools, vec!["read_file"]);
    }
    
    #[test]
    fn test_delegation_result() {
        let success = DelegationResult::success("del_123".to_string(), "Done".to_string(), 3);
        assert!(success.success);
        assert_eq!(success.iterations_used, 3);
        
        let failure = DelegationResult::failure("del_456".to_string(), "Timeout".to_string());
        assert!(!failure.success);
        assert_eq!(failure.error, Some("Timeout".to_string()));
    }
}
