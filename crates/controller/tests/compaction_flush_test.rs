use chrono::Utc;
use multi_agent_controller::capability::{AgentCapability, CompressionCapability};
use multi_agent_controller::context::{CompressionConfig, TruncationCompressor};
use multi_agent_core::types::{HistoryEntry, Session, SessionStatus, TaskState, TokenUsage};
use std::fs;
use std::sync::Arc;
use uuid::Uuid;

fn make_session_with_history(count: usize) -> Session {
    let mut history = Vec::new();
    history.push(HistoryEntry {
        role: "system".to_string(),
        content: Arc::new("You are a helpful assistant".to_string()),
        tool_call: None,
        timestamp: Utc::now().timestamp(),
    });
    for i in 0..count {
        history.push(HistoryEntry {
            role: if i % 2 == 0 {
                "user".to_string()
            } else {
                "assistant".to_string()
            },
            content: Arc::new(format!("Message {} {}", i, "x".repeat(120))),
            tool_call: None,
            timestamp: Utc::now().timestamp(),
        });
    }

    Session {
        id: format!("sess-{}", Uuid::new_v4()),
        trace_id: Uuid::new_v4().to_string(),
        user_id: Some("tester".to_string()),
        history,
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        status: SessionStatus::Running,
        token_usage: TokenUsage::with_budget(10_000),
        task_state: Some(TaskState {
            goal: "Compaction validation".to_string(),
            iteration: 0,
            observations: Vec::new(),
            pending_actions: Vec::new(),
            consecutive_rejections: 0,
        }),
    }
}

#[tokio::test]
async fn test_pre_compaction_flush_and_history_compaction() {
    let dir = std::env::temp_dir().join(format!("ma_compact_{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    unsafe {
        std::env::set_var("MULTI_AGENT_MEMORY_DIR", dir.to_string_lossy().to_string());
    }

    let config = CompressionConfig {
        max_tokens: 100,
        trigger_threshold: 0.2,
        target_ratio: 0.5,
        preserve_recent: 3,
    };
    let cap = CompressionCapability::new(Arc::new(TruncationCompressor::new()), config);

    let mut session = make_session_with_history(20);
    let before = session.history.len();
    cap.on_pre_reasoning(&mut session).await.unwrap();
    let after = session.history.len();
    assert!(after < before, "history should be compacted");

    let daily = dir.join(format!("{}.md", Utc::now().format("%Y-%m-%d")));
    let text = fs::read_to_string(daily).unwrap();
    assert!(
        text.contains("PRE-COMPACTION"),
        "pre-compaction checkpoint should be flushed"
    );
}
