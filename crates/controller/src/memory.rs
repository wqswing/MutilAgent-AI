//! Capability for Long-Team Memory (RAG).
//!
//! This capability injects relevant memories into the context at the start of a task
//! and archives the execution result upon completion.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;

use multi_agent_core::{
    traits::{MemoryStore, MemoryEntry, LlmClient},
    types::{Session, AgentResult, HistoryEntry},
    Result, Error,
};
use crate::capability::AgentCapability;

/// Capability for Long-Term Memory (RAG).
pub struct MemoryCapability {
    /// The vector store for memory.
    store: Arc<dyn MemoryStore>,
    /// The LLM client for generating embeddings.
    llm: Arc<dyn LlmClient>,
    /// Number of memories to inject.
    limit: usize,
    /// Minimum similarity score (0.0 to 1.0) - logic to be added in search.
    _threshold: f32,
    /// Cached plan/goal to archive on finish.
    current_goal: Mutex<Option<String>>,
}

impl MemoryCapability {
    /// Create a new MemoryCapability.
    pub fn new(
        store: Arc<dyn MemoryStore>,
        llm: Arc<dyn LlmClient>,
        limit: usize,
        threshold: f32,
    ) -> Self {
        Self {
            store,
            llm,
            limit,
            _threshold: threshold,
            current_goal: Mutex::new(None),
        }
    }

    async fn retrieve_context(&self, goal: &str) -> Result<Vec<MemoryEntry>> {
        // 1. Generate embedding for the goal
        let embedding = self.llm.embed(goal).await
            .map_err(|e| Error::controller(format!("Failed to embed goal: {}", e)))?;

        // 2. Search memory
        self.store.search(&embedding, self.limit).await
    }
}

#[async_trait]
impl AgentCapability for MemoryCapability {
    fn name(&self) -> &str {
        "long_term_memory"
    }

    async fn on_start(&self, session: &mut Session) -> Result<()> {
        let goal = if let Some(state) = &session.task_state {
            state.goal.clone()
        } else {
            return Ok(());
        };

        // Cache goal for archiving later
        *self.current_goal.lock().await = Some(goal.clone());

        tracing::info!(goal = %goal, "Searching memory for context");
        match self.retrieve_context(&goal).await {
            Ok(memories) => {
                if !memories.is_empty() {
                    let mut context_msg = "Here are some relevant past experiences found in long-term memory:\n\n".to_string();
                    for (i, mem) in memories.iter().enumerate() {
                        context_msg.push_str(&format!("{}. {}\n", i + 1, mem.content));
                    }
                    context_msg.push_str("\n\nUse these insights to solve the current task more effectively.");

                    // Inject as system message (or pseudo-system user message)
                    session.history.push(HistoryEntry {
                        role: "system".to_string(), // Or user with strict instruction
                        content: Arc::new(context_msg),
                        tool_call: None,
                        timestamp: Utc::now().timestamp(),
                    });
                     tracing::info!("Injected {} memories into context", memories.len());
                }
            }
            Err(e) => {
                tracing::warn!("Failed to retrieve memory context: {}", e);
                // Don't fail the session, just continue without memory
            }
        }
        Ok(())
    }

    async fn on_finish(&self, session: &mut Session, result: &AgentResult) -> Result<()> {
         // Only archive successful missions
         // Note: result here is the final output. We might want to summarize the *whole* session.
         // For now, simplistically archive: "Goal: [goal] -> Result: [output]"

        let goal = {
            let guard = self.current_goal.lock().await;
            guard.clone()
        };

        if let Some(goal_text) = goal {
             // Create content to store
             let content = match result {
                 AgentResult::Text(text) => format!("Goal: {}\nResult: {}", goal_text, text),
                 AgentResult::Data(val) => format!("Goal: {}\nResult Data: {}", goal_text, val),
                 AgentResult::File { filename, .. } => format!("Goal: {}\nResult File: {}", goal_text, filename),
                 _ => return Ok(()),
             };

             tracing::info!("Archiving experience to memory");

             // Embed
             let embedding = self.llm.embed(&content).await
                 .map_err(|e| Error::controller(format!("Failed to embed experience: {}", e)))?;

             // Store
             let entry = MemoryEntry {
                 id: uuid::Uuid::new_v4().to_string(),
                 content,
                 embedding,
                 metadata: std::collections::HashMap::from([
                     ("type".to_string(), "experience".to_string()),
                     ("session_id".to_string(), session.id.clone()),
                     ("timestamp".to_string(), Utc::now().to_rfc3339()),
                 ]),
             };

             if let Err(e) = self.store.add(entry).await {
                 tracing::warn!("Failed to save experience to memory: {}", e);
             }
        }

        Ok(())
    }
}
