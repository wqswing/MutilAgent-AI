//! Summarization Capability — Post-Task Knowledge Extraction.
//!
//! This capability hooks into the agent's `on_finish` lifecycle to:
//! 1. Ask the LLM to generate a structured summary of the completed task.
//! 2. Generate an embedding for the summary.
//! 3. Store the summary in the `KnowledgeStore` for future retrieval.
//!
//! It also hooks into `on_start` to retrieve relevant past knowledge
//! and inject it into the session context.

use async_trait::async_trait;
use std::sync::Arc;

use multi_agent_core::{
    traits::{ChatMessage, KnowledgeEntry, KnowledgeStore, LlmClient},
    types::{AgentResult, HistoryEntry, Session},
    Result,
};

use crate::capability::AgentCapability;

/// Summarization capability for post-task knowledge extraction.
pub struct SummarizationCapability {
    /// Knowledge store for persisting summaries.
    knowledge_store: Arc<dyn KnowledgeStore>,
    /// LLM client for generating summaries and embeddings.
    llm: Arc<dyn LlmClient>,
    /// Maximum number of knowledge entries to retrieve for context.
    max_context_entries: usize,
    /// Embedding dimensionality (depends on the LLM model used).
    embedding_dim: usize,
}

impl SummarizationCapability {
    /// Create a new summarization capability.
    pub fn new(knowledge_store: Arc<dyn KnowledgeStore>, llm: Arc<dyn LlmClient>) -> Self {
        Self {
            knowledge_store,
            llm,
            max_context_entries: 5,
            embedding_dim: 64,
        }
    }

    /// Set the maximum number of knowledge entries to retrieve for context.
    pub fn with_max_context(mut self, max: usize) -> Self {
        self.max_context_entries = max;
        self
    }

    /// Set the embedding dimension.
    pub fn with_embedding_dim(mut self, dim: usize) -> Self {
        self.embedding_dim = dim;
        self
    }

    /// Generate a simple bag-of-words embedding from text.
    ///
    /// This is a lightweight fallback when no external embedding model is available.
    /// For production, replace with a proper embedding API (OpenAI, Cohere, etc.).
    fn simple_embedding(&self, text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0f32; self.embedding_dim];
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return embedding;
        }
        for word in &words {
            let hash = Self::string_hash(word);
            let idx = (hash as usize) % self.embedding_dim;
            embedding[idx] += 1.0;
        }
        // Normalize
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for v in &mut embedding {
                *v /= magnitude;
            }
        }
        embedding
    }

    /// Simple but deterministic string hash.
    fn string_hash(s: &str) -> u32 {
        let mut hash: u32 = 5381;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        hash
    }

    /// Build the summarization prompt from session history.
    fn build_summary_prompt(session: &Session) -> String {
        let goal = session
            .task_state
            .as_ref()
            .map(|t| t.goal.as_str())
            .unwrap_or("unknown");

        let observations: Vec<String> = session
            .task_state
            .as_ref()
            .map(|t| {
                t.observations
                    .iter()
                    .take(10)
                    .map(|o| format!("- {}", &o[..o.len().min(200)]))
                    .collect()
            })
            .unwrap_or_default();

        let tools_used: Vec<String> = session
            .history
            .iter()
            .filter_map(|h| h.tool_call.as_ref().map(|tc| tc.name.clone()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        format!(
            r#"Summarize the following completed agent task into a concise knowledge entry.
Focus on: what was accomplished, key decisions, tools used, and lessons learned.
Keep it under 200 words.

TASK GOAL: {}
TOOLS USED: {}
KEY OBSERVATIONS:
{}

Respond with ONLY the summary text, no formatting or prefixes."#,
            goal,
            tools_used.join(", "),
            observations.join("\n")
        )
    }

    /// Extract tags from the summary text.
    fn extract_tags(summary: &str, goal: &str) -> Vec<String> {
        let mut tags = Vec::new();

        let keyword_tags = [
            ("code", "coding"),
            ("file", "filesystem"),
            ("database", "database"),
            ("api", "api"),
            ("error", "debugging"),
            ("test", "testing"),
            ("deploy", "deployment"),
            ("config", "configuration"),
            ("security", "security"),
            ("sandbox", "sandbox"),
        ];

        let lower = format!("{} {}", summary, goal).to_lowercase();
        for (keyword, tag) in &keyword_tags {
            if lower.contains(keyword) {
                tags.push(tag.to_string());
            }
        }

        tags.push("auto-summarized".to_string());
        tags
    }
}

#[async_trait]
impl AgentCapability for SummarizationCapability {
    fn name(&self) -> &str {
        "knowledge_summarization"
    }

    /// On task start: retrieve relevant past knowledge and inject into context.
    async fn on_start(&self, session: &mut Session) -> Result<()> {
        let goal = match session.task_state.as_ref() {
            Some(task) => task.goal.clone(),
            None => return Ok(()),
        };

        let query_embedding = self.simple_embedding(&goal);

        let related = self
            .knowledge_store
            .search(&query_embedding, self.max_context_entries)
            .await?;

        if related.is_empty() {
            tracing::debug!("No relevant past knowledge found for goal");
            return Ok(());
        }

        let knowledge_context: Vec<String> = related
            .iter()
            .map(|k| format!("• [{}] {}", k.source_task, k.summary))
            .collect();

        let injection = format!(
            "RELEVANT PAST KNOWLEDGE ({} entries):\n{}\n\nUse this knowledge to inform your approach.",
            related.len(),
            knowledge_context.join("\n")
        );

        session.history.push(HistoryEntry {
            role: "user".to_string(),
            content: Arc::new(injection),
            tool_call: None,
            timestamp: chrono::Utc::now().timestamp(),
        });

        tracing::info!(
            count = related.len(),
            "Injected relevant past knowledge into session context"
        );

        Ok(())
    }

    /// On task finish: summarize the task and store as knowledge.
    async fn on_finish(&self, session: &mut Session, _result: &AgentResult) -> Result<()> {
        let goal = session
            .task_state
            .as_ref()
            .map(|t| t.goal.clone())
            .unwrap_or_default();

        if goal.is_empty() {
            tracing::debug!("Skipping summarization: no task goal");
            return Ok(());
        }

        let prompt = Self::build_summary_prompt(session);

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
            tool_calls: None,
        }];

        let summary_text = match self.llm.chat(&messages).await {
            Ok(response) => response.content,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to generate task summary — using fallback");
                format!("Task completed: {}", goal)
            }
        };

        let embedding = self.simple_embedding(&summary_text);
        let tags = Self::extract_tags(&summary_text, &goal);

        let entry = KnowledgeEntry {
            id: uuid::Uuid::new_v4().to_string(),
            summary: summary_text,
            source_task: goal,
            session_id: session.id.clone(),
            embedding,
            tags,
            created_at: chrono::Utc::now().timestamp(),
        };

        match self.knowledge_store.store(entry).await {
            Ok(id) => {
                tracing::info!(
                    knowledge_id = %id,
                    "Task knowledge summarized and stored"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to store task knowledge");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multi_agent_core::traits::{LlmResponse, LlmUsage};
    use multi_agent_core::types::{SessionStatus, TaskState, TokenUsage};
    use multi_agent_store::InMemoryKnowledgeStore;

    /// Simple mock LLM for testing.
    struct MockSummaryLlm;

    #[async_trait]
    impl LlmClient for MockSummaryLlm {
        async fn complete(&self, _prompt: &str) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: "mock".to_string(),
                finish_reason: "stop".to_string(),
                usage: LlmUsage::default(),
                tool_calls: None,
            })
        }

        async fn chat(&self, _messages: &[ChatMessage]) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: "Successfully analyzed the codebase and identified key security patterns."
                    .to_string(),
                finish_reason: "stop".to_string(),
                usage: LlmUsage {
                    prompt_tokens: 100,
                    completion_tokens: 30,
                    total_tokens: 130,
                },
                tool_calls: None,
            })
        }

        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1; 64])
        }
    }

    fn create_test_session() -> Session {
        Session {
            id: "test-session-42".to_string(),
            trace_id: "test-trace-42".to_string(),
            user_id: None,
            status: SessionStatus::Running,
            history: vec![],
            task_state: Some(TaskState {
                iteration: 0,
                goal: "Analyze the Rust codebase for security vulnerabilities".to_string(),
                observations: vec![
                    Arc::new("Found 3 unsafe blocks".to_string()),
                    Arc::new("No SQL injection risks".to_string()),
                ],
                pending_actions: vec![],
                consecutive_rejections: 0,
            }),
            token_usage: TokenUsage::default(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    #[tokio::test]
    async fn test_summarization_on_finish() {
        let store = Arc::new(InMemoryKnowledgeStore::new());
        let llm = Arc::new(MockSummaryLlm);

        let cap = SummarizationCapability::new(store.clone(), llm);

        let mut session = create_test_session();
        let result = AgentResult::Text("Done".into());

        cap.on_finish(&mut session, &result).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_knowledge_retrieval_on_start() {
        let store = Arc::new(InMemoryKnowledgeStore::new());
        let llm = Arc::new(MockSummaryLlm);

        let cap = SummarizationCapability::new(store.clone(), llm.clone());

        // First: complete a task that generates knowledge
        let mut session1 = create_test_session();
        let result = AgentResult::Text("Done".into());
        cap.on_finish(&mut session1, &result).await.unwrap();

        // Second: start a new similar task
        let mut session2 = Session {
            id: "test-session-43".to_string(),
            trace_id: "test-trace-43".to_string(),
            user_id: None,
            status: SessionStatus::Running,
            history: vec![],
            task_state: Some(TaskState {
                iteration: 0,
                goal: "Scan Rust code for security issues".to_string(),
                observations: vec![],
                pending_actions: vec![],
                consecutive_rejections: 0,
            }),
            token_usage: TokenUsage::default(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        let history_before = session2.history.len();
        cap.on_start(&mut session2).await.unwrap();

        // Should have injected knowledge context
        assert!(session2.history.len() > history_before);
        let last_entry = session2.history.last().unwrap();
        assert!(last_entry.content.contains("RELEVANT PAST KNOWLEDGE"));
    }

    #[test]
    fn test_extract_tags() {
        let tags = SummarizationCapability::extract_tags(
            "Fixed a security vulnerability in the API endpoint by adding proper file validation",
            "Fix API security issue",
        );

        assert!(tags.contains(&"security".to_string()));
        assert!(tags.contains(&"api".to_string()));
        assert!(tags.contains(&"filesystem".to_string()));
        assert!(tags.contains(&"auto-summarized".to_string()));
    }

    #[test]
    fn test_simple_embedding() {
        let cap = SummarizationCapability::new(
            Arc::new(InMemoryKnowledgeStore::new()),
            Arc::new(MockSummaryLlm),
        );

        let emb1 = cap.simple_embedding("rust security analysis");
        let emb2 = cap.simple_embedding("rust security analysis");
        let emb3 = cap.simple_embedding("python web development");

        assert_eq!(emb1, emb2);
        assert_ne!(emb1, emb3);
        let mag: f32 = emb1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((mag - 1.0).abs() < 0.001);
    }
}
