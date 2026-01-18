//! Agent Capabilities System.
//!
//! This module defines the `AgentCapability` trait, which allows extending the
//! `ReActController` with modular capabilities (plugins) without modifying the core logic.
//!
//! Capabilities can hook into the agent's lifecycle:
//! - `on_start`: Called when a task begins.
//! - `on_pre_reasoning`: Called before sending history to the LLM (e.g., compression, security).
//! - `on_instruction`: Called to parse custom instructions from the LLM response.
//! - `on_execute`: Called to execute custom actions.

use async_trait::async_trait;
use std::sync::Arc;
use mutil_agent_core::{Result, Error};
use mutil_agent_core::types::{Session, AgentResult, HistoryEntry};
use crate::parser::ReActAction;
use chrono::Utc; // Ensure chrono is available or use via core if re-exported

/// A pluggable capability for the agent.
#[async_trait]
pub trait AgentCapability: Send + Sync {
    /// Unique name of the capability.
    fn name(&self) -> &str;

    /// Called when a new task starts.
    /// Useful for initializing state or validating the goal.
    async fn on_start(&self, _session: &mut Session) -> Result<()> {
        Ok(())
    }

    /// Called before the agent reasons (calls the LLM).
    /// Useful for context management, security scanning, etc.
    async fn on_pre_reasoning(&self, _session: &mut Session) -> Result<()> {
        Ok(())
    }

    /// Called to parse a raw LLM response into an action.
    /// Returns `Some(Action)` if this capability recognizes the pattern.
    fn parse_action(&self, _response: &str) -> Option<ReActAction> {
        None
    }

    /// Called when the agent decides to execute a specific action.
    /// Returns `Some(Result)` if this capability handled the action.
    async fn on_execute(
        &self,
        _action: &ReActAction,
        _session: &mut Session,
    ) -> Result<Option<AgentResult>> {
        Ok(None)
    }

    /// Called after the agent has executed an action and observed the result.
    /// Useful for reflection, loop detection, or auto-correction.
    async fn on_post_execute(&self, _session: &mut Session) -> Result<()> {
        Ok(())
    }

    /// Hook called after the entire task is finished (e.g., for archiving).
    async fn on_finish(&self, _session: &mut Session, _result: &AgentResult) -> Result<()> {
        Ok(())
    }
}

// =============================================================================
// Capability Wrappers
// =============================================================================

/// Wrapper for Context Compression.
pub struct CompressionCapability {
    compressor: Arc<dyn crate::context::ContextCompressor>,
    config: crate::context::CompressionConfig,
}

impl CompressionCapability {
    pub fn new(
        compressor: Arc<dyn crate::context::ContextCompressor>,
        config: crate::context::CompressionConfig,
    ) -> Self {
        Self { compressor, config }
    }
}

#[async_trait]
impl AgentCapability for CompressionCapability {
    fn name(&self) -> &str {
        "context_compression"
    }

    async fn on_pre_reasoning(&self, session: &mut Session) -> Result<()> {
        let messages = crate::react::ReActController::build_messages_static(session);
        if self.compressor.needs_compression(&messages, &self.config) {
            tracing::info!("Capability triggering context compression");
            let _result = self.compressor.compress(messages, &self.config).await?;
            
            // Reconstruct history from compressed messages
            // This is complex because we need to map back to HistoryEntry
            // For now, simpler approach: just log it happened, as true integration 
            // requires deep controller changes. 
            // BETTER: The compressor should modify the session directly in v0.3 refactor.
            // For now, we'll keep the logic in the controller until we refactor build_messages.
        }
        Ok(())
    }
}

/// Wrapper for Security Guardrails.
pub struct SecurityCapability {
    guardrail: Arc<dyn mutil_agent_governance::Guardrail>,
}

impl SecurityCapability {
    pub fn new(guardrail: Arc<dyn mutil_agent_governance::Guardrail>) -> Self {
        Self { guardrail }
    }
}

#[async_trait]
impl AgentCapability for SecurityCapability {
    fn name(&self) -> &str {
        "security_guardrails"
    }

    async fn on_start(&self, session: &mut Session) -> Result<()> {
        // Check goal (initial input) for security violations
        if let Some(ref task_state) = session.task_state {
             let check = self.guardrail.check_input(&task_state.goal).await?;
             if !check.passed {
                 return Err(Error::controller(format!(
                     "Security violation: {}",
                     check.reason.unwrap_or_default()
                 )));
             }
        }
        Ok(())
    }

    async fn on_pre_reasoning(&self, session: &mut Session) -> Result<()> {
        // Check last user message
        if let Some(last_user_msg) = session.history.iter().rev().find(|e| e.role == "user") {
            let check = self.guardrail.check_input(&last_user_msg.content).await?;
            if !check.passed {
                return Err(Error::controller(format!(
                    "Security violation: {}",
                    check.reason.unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    async fn on_execute(
        &self,
        action: &ReActAction,
        _session: &mut Session,
    ) -> Result<Option<AgentResult>> {
        if let ReActAction::FinalAnswer(answer) = action {
            let check = self.guardrail.check_output(answer).await?;
            if !check.passed {
                return Err(Error::controller(format!(
                    "Output security violation: {}",
                    check.reason.unwrap_or_default()
                )));
            }
        }
        Ok(None)
    }
}

/// Wrapper for Delegation.
pub struct DelegationCapability {
    delegator: Arc<dyn crate::delegation::Delegator>,
}

impl DelegationCapability {
    pub fn new(delegator: Arc<dyn crate::delegation::Delegator>) -> Self {
        Self { delegator }
    }
}

#[async_trait]
impl AgentCapability for DelegationCapability {
    fn name(&self) -> &str {
        "subagent_delegation"
    }

    fn parse_action(&self, response: &str) -> Option<ReActAction> {
        if response.contains("DELEGATE:") {
            if let Some((_, rest)) = response.split_once("DELEGATE:") {
                let objective = rest.lines().next().unwrap_or("").trim().to_string();
                let context = if let Some(ctx_pos) = rest.find("CONTEXT:") {
                    rest[ctx_pos + 8..].lines().next().unwrap_or("").trim().to_string()
                } else {
                    String::new()
                };
                return Some(ReActAction::Delegate { objective, context });
            }
        }
        None
    }

    async fn on_execute(
        &self,
        action: &ReActAction,
        _session: &mut Session,
    ) -> Result<Option<AgentResult>> {
        if let ReActAction::Delegate { objective, context } = action {
             let request = crate::delegation::DelegationRequest::new(objective)
                .with_context(context);
            
            let result = self.delegator.delegate(request).await?;
            if result.success {
                Ok(Some(AgentResult::Text(format!("Subagent completed: {}", result.result))))
            } else {
                Ok(Some(AgentResult::Text(format!("Subagent failed: {}", result.error.unwrap_or_default()))))
            }
        } else {
            Ok(None)
        }
    }
}

/// Wrapper for MCP Registry (autonomous selection).
pub struct McpCapability {
    registry: Arc<mutil_agent_skills::McpRegistry>,
}

impl McpCapability {
    pub fn new(registry: Arc<mutil_agent_skills::McpRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl AgentCapability for McpCapability {
    fn name(&self) -> &str {
        "mcp_autonomous_selection"
    }

    fn parse_action(&self, response: &str) -> Option<ReActAction> {
        if response.contains("MCP_SELECT:") {
            if let Some((_, rest)) = response.split_once("MCP_SELECT:") {
                let task_description = rest.lines().next().unwrap_or("").trim().to_string();
                return Some(ReActAction::McpSelect { task_description });
            }
        }
        None
    }

    async fn on_execute(
        &self,
        action: &ReActAction,
        _session: &mut Session,
    ) -> Result<Option<AgentResult>> {
        if let ReActAction::McpSelect { task_description } = action {
            tracing::info!(task = %task_description, "Selecting MCP server via capability");
            
            let observation = match self.registry.select_for_task(task_description) {
                Some(server) => {
                    match self.registry.connect_server(&server.id).await {
                        Ok(()) => format!(
                            "Selected and connected to MCP server '{}' ({}). Capabilities: {:?}. You can now use tools from this server.",
                            server.name, server.id, server.capabilities
                        ),
                        Err(e) => format!("Connection failed: {}", e),
                    }
                }
                None => format!(
                    "No suitable MCP server found for: '{}'. Available: {:?}",
                    task_description,
                    self.registry.list_all().iter().map(|s| &s.name).collect::<Vec<_>>()
                ),
            };
            
            Ok(Some(AgentResult::Text(observation)))
        } else {
            Ok(None)
        }
    }
}

/// Capability for Self-Correction and Loop Detection.
pub struct ReflectionCapability {
    /// Limit of repetitive actions before triggering a warning
    threshold: usize,
}

impl ReflectionCapability {
    pub fn new(threshold: usize) -> Self {
        Self { threshold }
    }

    /// Check for repetitive tool calls
    fn detect_tool_loop(&self, session: &Session) -> Option<String> {
        let history = &session.history;
        // Determine if we have enough history to detect a loop
        // We need at least 'threshold' entries, not necessarily * 2
        if history.len() < self.threshold {
            return None;
        }

        // Look at recent tool calls
        let mut recent_tools = Vec::new();
        for entry in history.iter().rev() {
            if let Some(ref tool_call) = entry.tool_call {
                recent_tools.push((tool_call.name.clone(), tool_call.arguments.to_string()));
            }
            if recent_tools.len() >= self.threshold {
                break;
            }
        }

        if recent_tools.len() < self.threshold {
            return None;
        }

        // Check if all recent tools are the same
        let first = &recent_tools[0];
        if recent_tools.iter().all(|t| t == first) {
            return Some(format!(
                "CRITICAL WARNING: You have called the tool '{}' with arguments '{}' {} times in a row. Stop looping. Analyze *why* it is failing or returning the same result. Try a different tool or approach immediately.",
                first.0, first.1, self.threshold
            ));
        }

        None
    }
}

#[async_trait]
impl AgentCapability for ReflectionCapability {
    fn name(&self) -> &str {
        "reflection_self_correction"
    }

    async fn on_post_execute(&self, session: &mut Session) -> Result<()> {
        // 1. Tool Loop Detection
        if let Some(warning) = self.detect_tool_loop(session) {
            tracing::warn!("Reflection triggered: Tool loop detected");
            // Inject system warning
            session.history.push(HistoryEntry {
                role: "user".to_string(), // Using user role to act as system instruction
                content: Arc::new(warning),
                tool_call: None,
                timestamp: Utc::now().timestamp(),
            });
        }

        // 2. Error Loop Detection (Future: Check for consecutive error results)

        Ok(())
    }
}
