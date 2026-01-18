use std::sync::Arc;
use async_trait::async_trait;
use multi_agent_core::traits::{LlmClient, LlmResponse, ChatMessage, MemoryStore, MemoryEntry};
use multi_agent_core::Result;
use multi_agent_controller::memory::MemoryCapability;
use multi_agent_controller::capability::AgentCapability;
use multi_agent_core::types::{Session, SessionStatus, TaskState, AgentResult};
use multi_agent_store::SimpleVectorStore;
use chrono::Utc;
use uuid::Uuid;

// --- Mock LLM ---
struct MockLlm;
#[async_trait]
impl LlmClient for MockLlm {
    async fn complete(&self, _prompt: &str) -> Result<LlmResponse> {
        unimplemented!()
    }
    async fn chat(&self, _messages: &[ChatMessage]) -> Result<LlmResponse> {
        unimplemented!()
    }
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        // Return a dummy embedding
        Ok(vec![0.1, 0.2, 0.3])
    }
}

#[tokio::test]
async fn test_memory_context_injection() -> Result<()> {
    // 1. Setup
    let store = Arc::new(SimpleVectorStore::new());
    let llm = Arc::new(MockLlm);
    let memory = MemoryCapability::new(store.clone(), llm.clone(), 3, 0.5);

    // 2. Add some memories manually
    let entry = MemoryEntry {
        id: "mem1".to_string(),
        content: "Refactoring ReActController was hard.".to_string(),
        embedding: vec![0.1, 0.2, 0.3], // Matches mock embedding perfectly
        metadata: Default::default(),
    };
    store.add(entry).await?;

    // 3. Create Session with a goal
    let mut session = Session {
        id: Uuid::new_v4().to_string(),
        history: Vec::new(),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        status: SessionStatus::Running,
        token_usage: Default::default(),
        task_state: Some(TaskState {
            goal: "Refactor the controller".to_string(),
            iteration: 0,
            observations: Vec::new(),
            pending_actions: Vec::new(),
        }),
    };

    // 4. Run on_start (should populate history)
    memory.on_start(&mut session).await?;

    // 5. Verify injection
    assert!(!session.history.is_empty());
    let system_msg = &session.history[0];
    assert_eq!(system_msg.role, "system");
    assert!(system_msg.content.contains("Refactoring ReActController was hard"));

    Ok(())
}

#[tokio::test]
async fn test_memory_archival() -> Result<()> {
    // 1. Setup
    let store = Arc::new(SimpleVectorStore::new());
    let llm = Arc::new(MockLlm);
    let memory = MemoryCapability::new(store.clone(), llm.clone(), 3, 0.5);

    // 2. Create Session
    let mut session = Session {
        id: "sess1".to_string(),
        history: Vec::new(),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        status: SessionStatus::Running,
        token_usage: Default::default(),
        task_state: Some(TaskState {
            goal: "Fix the bug".to_string(), // This is cached in on_start
            iteration: 0,
            observations: Vec::new(),
            pending_actions: Vec::new(),
        }),
    };

    // Call on_start to cache the goal
    memory.on_start(&mut session).await?;

    // 3. Simulate Finish
    let result = AgentResult::Text("Bug fixed successfully.".to_string());
    memory.on_finish(&mut session, &result).await?;

    // 4. Verify Archival (Search for it)
    // Since mock embed returns [0.1, 0.2, 0.3], searching with same should find it
    let results = store.search(&[0.1, 0.2, 0.3], 1).await?;
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Goal: Fix the bug"));
    assert!(results[0].content.contains("Result: Bug fixed successfully"));

    Ok(())
}
