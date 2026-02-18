use chrono::Utc;
use multi_agent_controller::capability::AgentCapability;
use multi_agent_controller::memory_writeback::MemoryWritebackCapability;
use multi_agent_core::types::{AgentResult, Session, SessionStatus, TaskState, TokenUsage};
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn make_session(session_id: &str, goal: &str) -> Session {
    Session {
        id: session_id.to_string(),
        trace_id: Uuid::new_v4().to_string(),
        user_id: Some("tester".to_string()),
        history: Vec::new(),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        status: SessionStatus::Running,
        token_usage: TokenUsage::with_budget(10_000),
        task_state: Some(TaskState {
            goal: goal.to_string(),
            iteration: 0,
            observations: Vec::new(),
            pending_actions: Vec::new(),
            consecutive_rejections: 0,
        }),
    }
}

#[tokio::test]
async fn test_memory_writeback_creates_daily_and_memory_files() {
    let dir = std::env::temp_dir().join(format!("ma_mem_{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();

    let capability = MemoryWritebackCapability::new(dir.clone());
    let mut session = make_session("sess-a", "Generate architecture review");
    capability
        .on_finish(
            &mut session,
            &AgentResult::Text("Review completed".to_string()),
        )
        .await
        .unwrap();

    let daily = dir.join(format!("{}.md", Utc::now().format("%Y-%m-%d")));
    let memory = dir.join("MEMORY.md");
    assert!(daily.exists(), "daily file should exist");
    assert!(memory.exists(), "MEMORY.md should exist");

    let daily_text = fs::read_to_string(daily).unwrap();
    let memory_text = fs::read_to_string(memory).unwrap();
    assert!(daily_text.contains("Generate architecture review"));
    assert!(memory_text.contains("Generate architecture review"));
}

#[tokio::test]
async fn test_memory_writeback_merge_deduplicates_entries() {
    let dir = std::env::temp_dir().join(format!("ma_mem_{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let capability = MemoryWritebackCapability::new(PathBuf::from(&dir));

    let mut session = make_session("sess-dedup", "Fix flaky tests");
    capability
        .on_finish(&mut session, &AgentResult::Text("Done".to_string()))
        .await
        .unwrap();
    capability
        .on_finish(&mut session, &AgentResult::Text("Done".to_string()))
        .await
        .unwrap();

    let memory = dir.join("MEMORY.md");
    let memory_text = fs::read_to_string(memory).unwrap();
    let count = memory_text.matches("session:sess-dedup").count();
    assert_eq!(count, 1, "duplicate session+goal entries should be merged");
}

#[tokio::test]
async fn test_memory_writeback_sqlite_projection() {
    let dir = std::env::temp_dir().join(format!("ma_mem_sqlite_{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("memory.db");
    unsafe {
        std::env::set_var(
            "MULTI_AGENT_MEMORY_SQLITE_PATH",
            db_path.to_string_lossy().to_string(),
        );
    }

    let capability = MemoryWritebackCapability::new(dir.clone());
    let mut session = make_session("sess-sqlite", "Ship P1 memory service");
    capability
        .on_finish(&mut session, &AgentResult::Text("Done".to_string()))
        .await
        .unwrap();
    capability
        .on_finish(&mut session, &AgentResult::Text("Done".to_string()))
        .await
        .unwrap();

    let conn = Connection::open(&db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM memory_records WHERE session_id = ?1")
        .unwrap();
    let count: i64 = stmt.query_row(["sess-sqlite"], |row| row.get(0)).unwrap();
    assert_eq!(count, 1, "sqlite backend should deduplicate line entries");

    let memory = dir.join("MEMORY.md");
    let memory_text = fs::read_to_string(memory).unwrap();
    assert!(memory_text.contains("session:sess-sqlite"));

    unsafe {
        std::env::remove_var("MULTI_AGENT_MEMORY_SQLITE_PATH");
    }
}
