use multi_agent_controller::chrono_timestamp;
use multi_agent_controller::{InMemorySessionStore, ReActController, SessionStore};
use multi_agent_core::traits::Controller;
use multi_agent_core::types::{HistoryEntry, Session, SessionStatus, TaskState, TokenUsage};
use std::sync::Arc;

#[tokio::test]
async fn test_resume_session() -> anyhow::Result<()> {
    // 1. Setup persistent store
    let session_store = Arc::new(InMemorySessionStore::new());

    // 2. Setup controller with store
    let controller = ReActController::builder()
        .with_session_store(session_store.clone())
        .build();

    // 3. Create a mock session that looks like it was interrupted
    let session_id = "test-resume-session-id";
    let session = Session {
        id: session_id.to_string(),
        trace_id: "test-trace-resume".to_string(),
        user_id: None,
        status: SessionStatus::Running,
        history: vec![
            HistoryEntry {
                role: "system".to_string(),
                content: Arc::new("System prompt".to_string()),
                tool_call: None,
                timestamp: chrono_timestamp(),
            },
            HistoryEntry {
                role: "user".to_string(),
                content: Arc::new("Do something".to_string()),
                tool_call: None,
                timestamp: chrono_timestamp(),
            },
        ],
        task_state: Some(TaskState {
            iteration: 0,
            goal: "Do something".to_string(),
            observations: vec![],
            pending_actions: vec![],
            consecutive_rejections: 0,
        }),
        token_usage: TokenUsage::default(),
        created_at: chrono_timestamp(),
        updated_at: chrono_timestamp(),
    };

    // 4. Save session manually to store
    session_store.save(&session).await?;

    // 5. Call resume
    let result = controller.resume(session_id, None).await?;

    // 6. Verify result
    // Since we provided no LLM, it falls back to mock execution
    match result {
        multi_agent_core::types::AgentResult::Text(text) => {
            assert!(text.contains("Mock ReAct execution"));
        }
        _ => panic!("Expected text result from mock execution"),
    }

    // 7. Verify session is marked completed in store
    let loaded = session_store.load(session_id).await?.unwrap();
    assert_eq!(loaded.status, SessionStatus::Completed);

    Ok(())
}
