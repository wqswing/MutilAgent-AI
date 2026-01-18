//! Capability for Plan-and-Solve (Advanced Planning).
//!
//! This capability prompts the agent to create a structured plan before execution
//! and keeps the agent focused on the current step.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

use multi_agent_core::{
    traits::LlmClient,
    types::{Session, HistoryEntry},
    Result, Error,
};
use crate::capability::AgentCapability;

/// A step in the execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: usize,
    pub description: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// Capability for managing execution plans.
pub struct PlanningCapability {
    llm: Arc<dyn LlmClient>,
    plan: Mutex<Option<Vec<PlanStep>>>,
}

impl PlanningCapability {
    pub fn new(llm: Arc<dyn LlmClient>) -> Self {
        Self {
            llm,
            plan: Mutex::new(None),
        }
    }

    /// Generate a plan from the goal using the LLM.
    async fn generate_plan(&self, goal: &str) -> Result<Vec<PlanStep>> {
        let prompt = format!(
            "You are an expert planner. Break down the following goal into a clear, numbered list of steps.\n\
            Goal: {}\n\
            Return ONLY the numbered list, nothing else. Example:\n\
            1. Research the topic\n\
            2. Write the code\n\
            3. Test the solution",
            goal
        );

        let response = self.llm.complete(&prompt).await
            .map_err(|e| Error::controller(format!("Failed to generate plan: {}", e)))?;

        let mut steps = Vec::new();
        for (i, line) in response.content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() { continue; }
            
            // Simple parsing: just take the line content
            // Remove "1. " prefix if present
            let description = if let Some((_, rest)) = line.split_once('.') {
                rest.trim().to_string()
            } else {
                line.to_string()
            };

            steps.push(PlanStep {
                id: i + 1,
                description,
                status: StepStatus::Pending,
            });
        }

        if steps.is_empty() {
             // Fallback if parsing fails or LLM is weird
             steps.push(PlanStep {
                 id: 1,
                 description: format!("Execute goal: {}", goal),
                 status: StepStatus::Pending,
             });
        }

        // Set first step to InProgress
        if let Some(first) = steps.first_mut() {
            first.status = StepStatus::InProgress;
        }

        Ok(steps)
    }

    fn format_plan(steps: &[PlanStep]) -> String {
        let mut out = String::from("Current Plan:\n");
        for step in steps {
            let mark = match step.status {
                StepStatus::Completed => "[x]",
                StepStatus::InProgress => "[>]", // Current
                StepStatus::Failed => "[!]",
                StepStatus::Pending => "[ ]",
            };
            out.push_str(&format!("{} {}. {}\n", mark, step.id, step.description));
        }
        out
    }
}

#[async_trait]
impl AgentCapability for PlanningCapability {
    fn name(&self) -> &str {
        "planning_and_solving"
    }

    async fn on_start(&self, session: &mut Session) -> Result<()> {
        let goal = if let Some(state) = &session.task_state {
            &state.goal
        } else {
            return Ok(());
        };

        tracing::info!("Generating plan for goal: {}", goal);
        let steps = self.generate_plan(goal).await?;
        
        let plan_str = Self::format_plan(&steps);
        tracing::info!("Generated Plan:\n{}", plan_str);

        // Store plan
        *self.plan.lock().await = Some(steps);

        // Inject initial plan into history
        session.history.push(HistoryEntry {
            role: "system".to_string(),
            content: Arc::new(format!("I have generated a plan for your goal. Follow this plan:\n\n{}", plan_str)),
            tool_call: None,
            timestamp: chrono::Utc::now().timestamp(),
        });

        Ok(())
    }

    async fn on_pre_reasoning(&self, session: &mut Session) -> Result<()> {
        // Inject current plan status just to remind the LLM
        let plan_guard = self.plan.lock().await;
        if let Some(steps) = &*plan_guard {
             // Find current step
             if let Some(current) = steps.iter().find(|s| s.status == StepStatus::InProgress) {
                 let reminder = format!(
                     "SYSTEM REMINDER: You are currently working on Step {}: \"{}\". Focus ONLY on this step.",
                     current.id, current.description
                 );
                 
                 // We don't want to pollute history permanently with reminders in every loop? 
                 // Actually, in ReAct history is appended. 
                 // Let's perform ephemeral injection? 
                 // "on_pre_reasoning" modifies session before LLM call.
                 // Ideally we should inject a system message at the END of history so it's fresh?
                 // Or we instruct the specific prompt builder to include it.
                 // For compatibility, we append a user/system message.
                 session.history.push(HistoryEntry {
                     role: "system".to_string(), // or user
                     content: Arc::new(reminder),
                     tool_call: None,
                     timestamp: chrono::Utc::now().timestamp(),
                 });
             }
        }
        Ok(())
    }

    // TODO: Implement parsing logic to detect when a step is done (e.g., "STEP_COMPLETE")
    // For now, we rely on the LLM to follow the plan implicitly, 
    // or we can add a tool `complete_step(id)`?
}
