use std::sync::Arc;
use async_trait::async_trait;
use multi_agent_core::traits::{LlmClient, LlmResponse, ChatMessage};
use multi_agent_core::LlmUsage;
use multi_agent_core::Result;
use multi_agent_controller::planning::PlanningCapability;
use multi_agent_controller::capability::AgentCapability;
use multi_agent_core::types::{Session, SessionStatus, TaskState};
use chrono::Utc;
use uuid::Uuid;

// --- Mock LLM ---
struct MockPlannerLlm;
#[async_trait]
impl LlmClient for MockPlannerLlm {
    async fn complete(&self, prompt: &str) -> Result<LlmResponse> {
        // Return a mocked plan
        if prompt.contains("expert planner") {
            Ok(LlmResponse {
                content: "1. Step One\n2. Step Two".to_string(),
                finish_reason: "stop".to_string(),
                usage: LlmUsage::default(),
                tool_calls: None,
            })
        } else {
            Ok(LlmResponse {
                content: "".to_string(),
                finish_reason: "stop".to_string(),
                usage: LlmUsage::default(),
                tool_calls: None,
            })
        }
    }
    // Implement other required methods with stubs
    async fn chat(&self, _messages: &[ChatMessage]) -> Result<LlmResponse> {
        unimplemented!()
    }
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
       Ok(vec![])
    }
}

#[tokio::test]
async fn test_planning_generation() -> Result<()> {
    // 1. Setup
    let llm = Arc::new(MockPlannerLlm);
    let planner = PlanningCapability::new(llm);

    // 2. Create Session
    let mut session = Session {
        id: Uuid::new_v4().to_string(),
        history: Vec::new(),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        status: SessionStatus::Running,
        token_usage: Default::default(),
        task_state: Some(TaskState {
            goal: "Build a house".to_string(),
            iteration: 0,
            observations: Vec::new(),
            pending_actions: Vec::new(),
        }),
    };

    // 3. on_start (Should generate plan)
    planner.on_start(&mut session).await?;

    // 4. Verify Plan Injection
    assert!(!session.history.is_empty());
    let sys_msg = &session.history[0];
    assert_eq!(sys_msg.role, "system");
    assert!(sys_msg.content.contains("Step One"));
    assert!(sys_msg.content.contains("Step Two"));

    // 5. on_pre_reasoning (Should remind current step)
    planner.on_pre_reasoning(&mut session).await?;
    
    // Check if reminder is added
    let last_msg = session.history.last().unwrap();
    assert!(last_msg.content.contains("Step 1"));
    assert!(last_msg.content.contains("Step One"));
    assert!(last_msg.content.contains("SYSTEM REMINDER"));

    Ok(())
}
