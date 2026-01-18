//! Tool executor for the ReAct controller.
//!
//! Handles the execution of tools and management of observations.

use std::sync::Arc;
use multi_agent_core::{
    traits::ToolRegistry,
    types::{Session, HistoryEntry, ToolCallInfo},
    Error, Result,
};
use crate::capability::AgentCapability;

/// Tool executor that wraps registry access and observation management.
pub struct ToolExecutor {
    tools: Option<Arc<dyn ToolRegistry>>,
    capabilities: Vec<Arc<dyn AgentCapability>>,
}

impl ToolExecutor {
    /// Create a new tool executor.
    pub fn new(
        tools: Option<Arc<dyn ToolRegistry>>,
        capabilities: Vec<Arc<dyn AgentCapability>>,
    ) -> Self {
        Self { tools, capabilities }
    }

    /// Execute a tool and update the session with the observation.
    pub async fn execute(
        &self,
        session: &mut Session,
        name: String,
        args: serde_json::Value,
    ) -> Result<String> {
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

        // Add observation to history
        session.history.push(HistoryEntry {
            role: "user".to_string(),
            content: Arc::new(format!("OBSERVATION: {}", observation)),
            tool_call: Some(ToolCallInfo {
                name: name.clone(),
                arguments: args,
                result: Some(Arc::new(observation.clone())),
            }),
            timestamp: crate::react::chrono_timestamp(),
        });

        // Update task state
        if let Some(ref mut task_state) = session.task_state {
            task_state.observations.push(Arc::new(observation.clone()));
        }

        // Run post-execute hooks
        for cap in &self.capabilities {
            cap.on_post_execute(session)
                .await
                .map_err(|e| Error::controller(e.to_string()))?;
        }

        Ok(observation)
    }

    /// Validate security for a fast action before execution.
    pub async fn validate_fast_action_security(
        &self,
        args: &serde_json::Value,
    ) -> Result<()> {
        for cap in &self.capabilities {
            if cap.name() == "security_guardrails" {
                let mut temp_session = Session {
                    id: "security_check".to_string(),
                    status: multi_agent_core::types::SessionStatus::Running,
                    history: vec![HistoryEntry {
                        role: "user".to_string(),
                        content: Arc::new(serde_json::to_string(args).unwrap_or_default()),
                        tool_call: None,
                        timestamp: crate::react::chrono_timestamp(),
                    }],
                    task_state: None,
                    token_usage: Default::default(),
                    created_at: crate::react::chrono_timestamp(),
                    updated_at: crate::react::chrono_timestamp(),
                };
                cap.on_pre_reasoning(&mut temp_session)
                    .await
                    .map_err(|e| Error::controller(e.to_string()))?;
            }
        }
        Ok(())
    }
}
