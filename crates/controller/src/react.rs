//! ReAct loop implementation.
//!
//! ReAct (Reason + Act) is the core control loop for the agent:
//! 1. Reason about the current state
//! 2. Choose an action (tool or respond)
//! 3. Execute the action
//! 4. Observe the result
//! 5. Repeat until done or max iterations
//!
//! v0.2 Autonomous Capabilities:
//! - Dynamic Context Compression (auto-compresses when token threshold exceeded)
//! - Subagent Delegation (allows spawning child agents for subtasks)

use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use multi_agent_core::{
    traits::{ChatMessage, Controller, LlmClient, LlmResponse, ToolRegistry, SessionStore},
    types::{AgentResult, HistoryEntry, Session, SessionStatus, TaskState, TokenUsage, UserIntent, ToolCallInfo},
    Error, Result,
};

use crate::capability::AgentCapability;

// v0.3: Security Integration
// (Guardrail unused in pure Controller struct if verified via capabilities)
// Keeping core imports minimal

/// ReAct controller configuration.
#[derive(Debug, Clone)]
pub struct ReActConfig {
    /// Maximum iterations before giving up.
    pub max_iterations: usize,
    /// Default token budget.
    pub default_budget: u64,
    /// Enable state persistence.
    pub persist_state: bool,
    /// Temperature for LLM calls.
    pub temperature: f32,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            default_budget: 50_000,
            persist_state: true,
            temperature: 0.7,
        }
    }
}

// Use the new parser module
use crate::parser::ReActAction;

/// ReAct controller for executing complex tasks.
pub struct ReActController {
    /// Configuration.
    pub(crate) config: ReActConfig,
    /// LLM client for reasoning.
    pub(crate) llm: Option<Arc<dyn LlmClient>>,
    /// Tool registry for actions.
    pub(crate) tools: Option<Arc<dyn ToolRegistry>>,
    /// Session store for persistence.
    pub(crate) session_store: Option<Arc<dyn SessionStore>>,
    /// Agent capabilities (Unification of Compression, Delegation, MCP, Security).
    pub(crate) capabilities: Vec<Arc<dyn AgentCapability>>,
}

impl ReActController {
    /// Create a new builder for ReActController.
    pub fn builder() -> crate::builder::ReActBuilder {
        crate::builder::ReActBuilder::new()
    }

    /// Create a new ReAct controller with default config (legacy support).
    pub fn new(config: ReActConfig) -> Self {
        Self {
            config,
            llm: None,
            tools: None,
            session_store: None,
            capabilities: Vec::new(),
        }
    }

    /// Create a new session.
    fn create_session(&self, goal: &str) -> Session {
        Session {
            id: Uuid::new_v4().to_string(),
            status: SessionStatus::Running,
            history: vec![HistoryEntry {
                role: "system".to_string(),
                content: Arc::new(self.build_system_prompt(goal)),
                tool_call: None,
                timestamp: chrono_timestamp(),
            }],
            task_state: Some(TaskState {
                iteration: 0,
                goal: goal.to_string(),
                observations: Vec::new(),
                pending_actions: Vec::new(),
            }),
            token_usage: TokenUsage::with_budget(self.config.default_budget),
            created_at: chrono_timestamp(),
            updated_at: chrono_timestamp(),
        }
    }

    /// Build the system prompt for the agent.
    fn build_system_prompt(&self, goal: &str) -> String {
        let tools_description = self.get_tools_description();
        
        format!(
            r#"You are an AI assistant that uses the ReAct (Reasoning + Acting) pattern.

GOAL: {goal}

AVAILABLE TOOLS:
{tools_description}

INSTRUCTIONS:
1. Think step by step about what needs to be done
2. Use tools when needed by responding with ACTION
3. After receiving tool results, continue reasoning
4. When done, provide your FINAL ANSWER

RESPONSE FORMAT:
Use exactly one of these formats in each response:

For thinking/reasoning:
THOUGHT: <your reasoning here>

For tool calls:
ACTION: <tool_name>
ARGS: <json arguments>

For final answer (when task is complete):
FINAL ANSWER: <your complete answer>

Always think before acting. Be concise and focused on the goal."#
        )
    }



    /// Get description of available tools (for system prompt building).
    fn get_tools_description(&self) -> String {
        // For the system prompt, we return a placeholder since we can't call async here.
        // The actual tools list is fetched async when executing.
        "Tools will be loaded when execution starts.".to_string()
    }

    /// Build chat messages from session history (static version for capabilities).
    pub fn build_messages_static(session: &Session) -> Vec<ChatMessage> {
        session
            .history
            .iter()
            .map(|entry| ChatMessage {
                role: entry.role.clone(),
                content: entry.content.to_string(),
                tool_calls: None,
            })
            .collect()
    }

    /// Build chat messages from session history.
    fn build_messages(&self, session: &Session) -> Vec<ChatMessage> {
        Self::build_messages_static(session)
    }

    /// Parse the LLM response to extract action.
    fn parse_action(&self, response: &str) -> ReActAction {
        crate::parser::ActionParser::new(self.capabilities.clone()).parse(response)
    }

    /// Execute a single ReAct iteration with LLM.
    async fn execute_iteration_with_llm(
        &self,
        session: &mut Session,
        iteration: usize,
    ) -> Result<Option<AgentResult>> {
        let llm = self.llm.as_ref().ok_or_else(|| {
            Error::controller("LLM client not configured")
        })?;

        tracing::info!(
            session_id = %session.id,
            iteration = iteration,
            history_len = session.history.len(),
            "Executing ReAct iteration"
        );

        // v0.3: Capabilities On-Pre-Reasoning Hook (Compression, Security, etc.)
        for cap in &self.capabilities {
            cap.on_pre_reasoning(session).await.map_err(|e| Error::controller(e.to_string()))?;
        }

        let messages = self.build_messages(session); // Rebuild messages after potential compression

        // Call LLM with (possibly compressed) messages
        let response: LlmResponse = llm.chat(&messages).await?;

        // Update token usage
        session.token_usage.add(
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
        );

        tracing::debug!(
            response_len = response.content.len(),
            tokens_used = session.token_usage.total_tokens,
            "LLM response received"
        );

        // Add assistant response to history
        session.history.push(HistoryEntry {
            role: "assistant".to_string(),
            content: Arc::new(response.content.clone()),
            tool_call: None,
            timestamp: chrono_timestamp(),
        });

        // Parse and execute action
        let action = self.parse_action(&response.content);

        match action {
            ReActAction::FinalAnswer(ref answer) => {
                // Check capabilities on execution (Security Output check)
                for cap in &self.capabilities {
                    if let Some(result) = cap.on_execute(&action, session).await? {
                         // If a capability interrupts/handles FinalAnswer (e.g., blocks it), return that result
                         // Standard security cap returns Err on violation, keeping this flow simple.
                         match result {
                             AgentResult::Error { .. } => return Ok(Some(result)),
                             _ => {} // Ignore other results for FinalAnswer
                         }
                    }
                }
                
                tracing::info!(answer_len = answer.len(), "Task completed with final answer");
                Ok(Some(AgentResult::Text(answer.clone())))
            }

            ReActAction::ToolCall { name, args } => {
                self.handle_tool_call(session, name, args).await
            }

            ReActAction::Think(thought) => {
                tracing::debug!(thought_len = thought.len(), "Agent thinking");
                
                // Ask the agent to take an action
                session.history.push(HistoryEntry {
                    role: "user".to_string(),
                    content: Arc::new("Please take an action using a tool, or provide your FINAL ANSWER if the task is complete.".to_string()),
                    tool_call: None,
                    timestamp: chrono_timestamp(),
                });

                // v0.4: Post-Execute Hook
                for cap in &self.capabilities {
                    cap.on_post_execute(session).await.map_err(|e| Error::controller(e.to_string()))?;
                }

                Ok(None) // Continue loop
            }


            // Fallback: Check custom capability actions
            _ => {
                for cap in &self.capabilities {
                     if let Some(result) = cap.on_execute(&action, session).await? {
                         // Add observation to history if returned
                         if let AgentResult::Text(observation) = &result {
                             session.history.push(HistoryEntry {
                                role: "user".to_string(),
                                content: Arc::new(format!("OBSERVATION: {}", observation)),
                                tool_call: None,
                                timestamp: chrono_timestamp(),
                            });
                             // Update task state
                            if let Some(ref mut task_state) = session.task_state {
                                task_state.observations.push(Arc::new(observation.clone()));
                            }
                         }
                         
                        // v0.4: Post-Execute Hook
                        for cap in &self.capabilities {
                            cap.on_post_execute(session).await.map_err(|e| Error::controller(e.to_string()))?;
                        }

                         return Ok(None); // Action handled, continue loop
                     }
                }
                 // If no capability handled it, default behavior (shouldn't happen if parsed correctly)
                 Ok(None)
            }
        }
    }

    /// Execute iteration (mock if no LLM, real if LLM configured).
    async fn execute_iteration(
        &self,
        session: &mut Session,
        iteration: usize,
    ) -> Result<Option<AgentResult>> {
        if self.llm.is_some() {
            self.execute_iteration_with_llm(session, iteration).await
        } else {
            // Mock implementation for testing without LLM
            tracing::info!(
                session_id = %session.id,
                iteration = iteration,
                "Executing ReAct iteration (mock - no LLM)"
            );

            Ok(Some(AgentResult::Text(format!(
                "Mock ReAct execution. Goal: {}. Configure LLM client for real execution.",
                session
                    .task_state
                    .as_ref()
                    .map(|t| t.goal.as_str())
                    .unwrap_or("unknown")
            ))))
        }
    }

    async fn persist_session(&self, session: &Session) {
        if self.config.persist_state {
            if let Some(store) = &self.session_store {
                if let Err(e) = store.save(session).await {
                    tracing::warn!(error = %e, "Failed to save session state");
                }
            }
        }
    }

    async fn validate_fast_action_security(&self, args: &serde_json::Value) -> Result<()> {
        for cap in &self.capabilities {
            if cap.name() == "security_guardrails" {
                let mut temp_session = self.create_session("fast_action_check");
                temp_session.history.push(HistoryEntry {
                    role: "user".to_string(),
                    content: Arc::new(serde_json::to_string(args).unwrap_or_default()),
                    tool_call: None,
                    timestamp: chrono_timestamp(),
                });
                cap.on_pre_reasoning(&mut temp_session).await.map_err(|e| Error::controller(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn handle_tool_call(
        &self,
        session: &mut Session,
        name: String,
        args: serde_json::Value,
    ) -> Result<Option<AgentResult>> {
        tracing::info!(tool = %name, "Executing tool call");

        let observation = if let Some(ref tools) = self.tools {
            match tools.execute(&name, args.clone()).await {
                Ok(output) => {
                    if output.success {
                        format!("Tool '{}' succeeded:\n{}", name, output.content)
                    } else {
                        format!("Tool '{}' failed:\n{}", name, output.content)
                    }
                }
                Err(e) => format!("Tool '{}' error: {}", name, e),
            }
        } else {
            format!("Tool '{}' not available (no tools configured)", name)
        };

        session.history.push(HistoryEntry {
            role: "user".to_string(),
            content: Arc::new(format!("OBSERVATION: {}", observation)),
            tool_call: Some(ToolCallInfo {
                name: name.clone(),
                arguments: args,
                result: Some(Arc::new(observation.clone())),
            }),
            timestamp: chrono_timestamp(),
        });

        if let Some(ref mut task_state) = session.task_state {
            task_state.observations.push(Arc::new(observation));
        }

        for cap in &self.capabilities {
            cap.on_post_execute(session).await.map_err(|e| Error::controller(e.to_string()))?;
        }

        Ok(None)
    }
}

#[async_trait]
impl Controller for ReActController {
    async fn execute(&self, intent: UserIntent) -> Result<AgentResult> {
        match intent {
            UserIntent::FastAction { tool_name, args } => {
                self.validate_fast_action_security(&args).await?;

                // Fast path: direct tool execution
                tracing::info!(tool = %tool_name, "Fast path execution");

                if let Some(ref tools) = self.tools {
                    match tools.execute(&tool_name, args).await {
                        Ok(output) => {
                            if output.success {
                                Ok(AgentResult::Text(output.content))
                            } else {
                                Ok(AgentResult::Error {
                                    message: output.content,
                                    code: "TOOL_ERROR".to_string(),
                                })
                            }
                        }
                        Err(e) => Ok(AgentResult::Error {
                            message: e.to_string(),
                            code: "TOOL_NOT_FOUND".to_string(),
                        }),
                    }
                } else {
                    Ok(AgentResult::Text(format!(
                        "Fast path: would execute tool '{}'. Tools not configured.",
                        tool_name
                    )))
                }
            }

            UserIntent::ComplexMission {
                goal,
                context_summary,
                visual_refs,
            } => {
                let mut session = self.create_session(&goal);
                
                // v0.3: Capability On-Start Hook
                for cap in &self.capabilities {
                    cap.on_start(&mut session).await.map_err(|e| Error::controller(e.to_string()))?;
                }

                // Add user context to history
                session.history.push(HistoryEntry {
                    role: "user".to_string(),
                    content: Arc::new(if visual_refs.is_empty() {
                        context_summary.clone()
                    } else {
                        format!("{}\n\nReferences: {:?}", context_summary, visual_refs)
                    }),
                    tool_call: None,
                    timestamp: chrono_timestamp(),
                });
                
                for cap in &self.capabilities {
                     cap.on_pre_reasoning(&mut session).await.map_err(|e| Error::controller(e.to_string()))?;
                }

                tracing::info!(
                    goal = %goal,
                    context_len = context_summary.len(),
                    refs_count = visual_refs.len(),
                    "Starting ReAct loop"
                );

                for iteration in 0..self.config.max_iterations {
                    if let Some(ref mut task_state) = session.task_state {
                        task_state.iteration = iteration;
                    }

                    match self.execute_iteration(&mut session, iteration).await? {
                        Some(result) => {
                            session.updated_at = chrono_timestamp();
                            session.status = SessionStatus::Completed;
                            self.persist_session(&session).await;
                            return Ok(result);
                        }
                        None => {
                            session.updated_at = chrono_timestamp();
                            self.persist_session(&session).await;

                            if session.token_usage.is_exceeded() {
                                session.status = SessionStatus::Failed;
                                return Err(Error::BudgetExceeded {
                                    used: session.token_usage.total_tokens,
                                    limit: session.token_usage.budget_limit,
                                });
                            }
                            continue;
                        }
                    }
                }

                session.status = SessionStatus::Failed;
                Err(Error::MaxIterationsExceeded(self.config.max_iterations))
            }
        }
    }



    async fn resume(&self, session_id: &str) -> Result<AgentResult> {
        tracing::warn!(session_id = session_id, "Resume not yet implemented");
        Err(Error::controller("Resume not yet implemented - coming in Phase 2 persistence"))
    }

    async fn cancel(&self, session_id: &str) -> Result<()> {
        tracing::info!(session_id = session_id, "Cancel requested");
        Ok(())
    }
}

/// Get current timestamp.
pub fn chrono_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_final_answer() {
        let controller = ReActController::new(ReActConfig::default());
        
        let response = "FINAL ANSWER: The result is 42.";
        let action = controller.parse_action(response);
        
        match action {
            ReActAction::FinalAnswer(answer) => {
                assert_eq!(answer, "The result is 42.");
            }
            _ => panic!("Expected FinalAnswer"),
        }
    }

    #[test]
    fn test_parse_tool_call() {
        let controller = ReActController::new(ReActConfig::default());
        
        let response = r#"THOUGHT: I need to calculate something.
ACTION: calculator
ARGS: {"operation": "add", "a": 5, "b": 3}"#;
        
        let action = controller.parse_action(response);
        
        match action {
            ReActAction::ToolCall { name, args } => {
                assert_eq!(name, "calculator");
                assert_eq!(args["operation"], "add");
            }
            _ => panic!("Expected ToolCall, got {:?}", action),
        }
    }

    #[test]
    fn test_parse_thought() {
        let controller = ReActController::new(ReActConfig::default());
        
        let response = "THOUGHT: I need to think about this more.";
        let action = controller.parse_action(response);
        
        match action {
            ReActAction::Think(thought) => {
                assert!(thought.contains("think about"));
            }
            _ => panic!("Expected Think"),
        }
    }

    #[tokio::test]
    async fn test_fast_action() {
        let controller = ReActController::new(ReActConfig::default());

        let intent = UserIntent::FastAction {
            tool_name: "test_tool".to_string(),
            args: serde_json::json!({"query": "test"}),
        };

        let result = controller.execute(intent).await.unwrap();
        match result {
            AgentResult::Text(text) => {
                assert!(text.contains("test_tool"));
            }
            _ => panic!("Expected Text result"),
        }
    }

    #[tokio::test]
    async fn test_complex_mission_mock() {
        let controller = ReActController::new(ReActConfig::default());

        let intent = UserIntent::ComplexMission {
            goal: "Test goal".to_string(),
            context_summary: "Test context".to_string(),
            visual_refs: vec![],
        };

        let result = controller.execute(intent).await.unwrap();
        match result {
            AgentResult::Text(text) => {
                assert!(text.contains("Mock ReAct"));
            }
            _ => panic!("Expected Text result"),
        }
    }
}
