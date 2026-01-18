use std::sync::Arc;
use multi_agent_controller::react::{ReActConfig, ReActController};
use multi_agent_core::types::{Session, SessionStatus};

#[tokio::test]
async fn test_reflection_loop_detection() -> anyhow::Result<()> {
    // 1. Setup Controller with Reflection (Threshold = 3)
    let config = ReActConfig::default();
    let _controller = ReActController::builder()
        .with_config(config)
        .with_reflection(3)
        .build();

    use multi_agent_controller::capability::{AgentCapability, ReflectionCapability};
    use chrono::Utc;
    use uuid::Uuid;
    use multi_agent_core::types::{HistoryEntry, ToolCallInfo};

    let reflection = ReflectionCapability::new(3);
    let mut session = Session {
        id: Uuid::new_v4().to_string(),
        history: Vec::new(),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        status: SessionStatus::Running,
        token_usage: Default::default(),
        task_state: Some(multi_agent_core::types::TaskState {
            goal: "test goal".to_string(),
            iteration: 0,
            observations: Vec::new(),
            pending_actions: Vec::new(),
        }),
    };
    
    // Fill history with 3 identical tool calls
    for _ in 0..3 {
        session.history.push(HistoryEntry {
            role: "assistant".to_string(),
            content: Arc::new("Calling tool...".to_string()),
            tool_call: Some(ToolCallInfo {
                name: "my_tool".to_string(),
                arguments: serde_json::json!({"arg": "val"}),
                result: Some("output".to_string().into()),
            }),
            timestamp: Utc::now().timestamp(),
        });
    }
    
    // 3. Trigger check
    reflection.on_post_execute(&mut session).await?;
    
    // 4. Verify warning injection
    let last_entry = session.history.last().unwrap();
    assert_eq!(last_entry.role, "user");
    assert!(last_entry.content.contains("CRITICAL WARNING"));
    assert!(last_entry.content.contains("3 times in a row"));
    
    println!("Reflection test passed: Warning injected correctly.");
    
    Ok(())
}
