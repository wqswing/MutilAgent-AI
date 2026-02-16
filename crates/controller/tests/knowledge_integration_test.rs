//! Knowledge retrieval integration tests.
//!
//! Tests the full pipeline: SummarizationCapability → InMemoryKnowledgeStore → Context Injection.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use multi_agent_controller::capability::AgentCapability;
use multi_agent_controller::summarization::SummarizationCapability;
use multi_agent_core::traits::{ChatMessage, KnowledgeStore, LlmClient, LlmResponse, LlmUsage};
use multi_agent_core::types::{AgentResult, Session, SessionStatus, TaskState, TokenUsage};
use multi_agent_core::Result;
use multi_agent_store::InMemoryKnowledgeStore;

// =============================================================================
// Mock LLM
// =============================================================================

struct MockSummaryLlm;

#[async_trait]
impl LlmClient for MockSummaryLlm {
    async fn complete(&self, _prompt: &str) -> Result<LlmResponse> {
        Ok(LlmResponse {
            content:
                "Analyzed codebase security. Found 3 unsafe blocks. Recommended fixes applied."
                    .to_string(),
            finish_reason: "stop".to_string(),
            usage: LlmUsage {
                prompt_tokens: 50,
                completion_tokens: 20,
                total_tokens: 70,
            },
            tool_calls: None,
        })
    }
    async fn chat(&self, _messages: &[ChatMessage]) -> Result<LlmResponse> {
        self.complete("").await
    }
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1; 64])
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn make_session(goal: &str) -> Session {
    Session {
        id: Uuid::new_v4().to_string(),
        trace_id: Uuid::new_v4().to_string(),
        user_id: None,
        status: SessionStatus::Running,
        history: Vec::new(),
        task_state: Some(TaskState {
            iteration: 0,
            goal: goal.to_string(),
            observations: vec![
                Arc::new("Found 3 unsafe blocks".to_string()),
                Arc::new("No SQL injection risks".to_string()),
            ],
            pending_actions: vec![],
            consecutive_rejections: 0,
        }),
        token_usage: TokenUsage::default(),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
    }
}

// =============================================================================
// 1. 任务完成后知识存储
// =============================================================================

#[tokio::test]
async fn test_knowledge_stored_after_task_completion() {
    let store = Arc::new(InMemoryKnowledgeStore::new());
    let llm = Arc::new(MockSummaryLlm);
    let cap = SummarizationCapability::new(store.clone(), llm);

    let mut session = make_session("Analyze Rust codebase for security");
    let result = AgentResult::Text("Analysis complete.".into());

    cap.on_finish(&mut session, &result).await.unwrap();

    // Knowledge store should have 1 entry
    let count = store.count().await.unwrap();
    assert_eq!(count, 1, "Should store exactly 1 knowledge entry");
}

// =============================================================================
// 2. 知识检索注入上下文
// =============================================================================

#[tokio::test]
async fn test_knowledge_injected_into_new_session() {
    let store = Arc::new(InMemoryKnowledgeStore::new());
    let llm = Arc::new(MockSummaryLlm);
    let cap = SummarizationCapability::new(store.clone(), llm.clone());

    // Phase 1: Complete a task → store knowledge
    let mut session1 = make_session("Analyze security vulnerabilities");
    let result = AgentResult::Text("Done".into());
    cap.on_finish(&mut session1, &result).await.unwrap();

    // Phase 2: Start a new related task → should get context injection
    let mut session2 = make_session("Review security audit results");
    let history_before = session2.history.len();

    cap.on_start(&mut session2).await.unwrap();

    assert!(
        session2.history.len() > history_before,
        "History should have new entries after on_start"
    );
    let last = session2.history.last().unwrap();
    assert!(
        last.content.contains("RELEVANT PAST KNOWLEDGE"),
        "Should contain knowledge injection marker"
    );
}

// =============================================================================
// 3. 知识存储后可通过标签搜索
// =============================================================================

#[tokio::test]
async fn test_knowledge_searchable_by_tags() {
    let store = Arc::new(InMemoryKnowledgeStore::new());
    let llm = Arc::new(MockSummaryLlm);
    let cap = SummarizationCapability::new(store.clone(), llm);

    // Complete a task → store knowledge (summary will contain "security")
    let mut session = make_session("Fix security vulnerability in API");
    let result = AgentResult::Text("Security fix applied.".into());
    cap.on_finish(&mut session, &result).await.unwrap();

    // Search by auto-summarized tag (always added)
    let results = store
        .search_by_tags(&["auto-summarized".to_string()], 10)
        .await
        .unwrap();
    assert!(
        !results.is_empty(),
        "Should find entries with auto-summarized tag"
    );
}

// =============================================================================
// 4. 空知识库不注入任何内容
// =============================================================================

#[tokio::test]
async fn test_empty_knowledge_store_no_injection() {
    let store = Arc::new(InMemoryKnowledgeStore::new());
    let llm = Arc::new(MockSummaryLlm);
    let cap = SummarizationCapability::new(store, llm);

    let mut session = make_session("Brand new task");
    let history_before = session.history.len();

    cap.on_start(&mut session).await.unwrap();

    assert_eq!(
        session.history.len(),
        history_before,
        "Empty knowledge store should not inject anything"
    );
}
