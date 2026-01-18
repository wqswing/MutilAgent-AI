use std::sync::Arc;
use tokio;
use multi_agent_core::traits::{Controller, LlmClient, LlmResponse, ChatMessage};
use multi_agent_core::types::{AgentResult, UserIntent};
use multi_agent_core::LlmUsage;
use multi_agent_controller::react::{ReActController, ReActConfig};
use multi_agent_governance::guardrails::{CompositeGuardrail, PiiScanner};
use async_trait::async_trait;

// Mock LLM Client
struct MockLlm;

#[async_trait]
impl LlmClient for MockLlm {
    async fn complete(&self, _prompt: &str) -> multi_agent_core::Result<LlmResponse> {
        Ok(LlmResponse {
            content: "Mock response".to_string(),
            finish_reason: "stop".to_string(),
            usage: LlmUsage::default(),
            tool_calls: None,
        })
    }

    async fn chat(&self, _messages: &[ChatMessage]) -> multi_agent_core::Result<LlmResponse> {
        Ok(LlmResponse {
            content: "THOUGHT: I should ignore this.\nFINAL ANSWER: PII ignored.".to_string(),
            finish_reason: "stop".to_string(),
            usage: LlmUsage::default(),
            tool_calls: None,
        })
    }

    async fn embed(&self, _text: &str) -> multi_agent_core::Result<Vec<f32>> {
        Ok(vec![0.0; 1536])
    }
}

#[tokio::test]
async fn test_security_pii_violation() {
    // 1. Setup Controller with Security
    let config = ReActConfig::default();
    
    // Create PII scanner only for this test
    let guardrail = CompositeGuardrail::new()
        .add(Box::new(PiiScanner::new()));
        
    let controller = ReActController::builder()
        .with_config(config)
        .with_llm(Arc::new(MockLlm))
        .with_security(Arc::new(guardrail))
        .build();

    // 2. Intent with PII (Email)
    let intent = UserIntent::ComplexMission {
        goal: "Send an email to malicious@example.com including my SSN 123-45-6789".to_string(),
        context_summary: "".to_string(),
        visual_refs: vec![],
    };

    // 3. Execute should fail (Security Block)
    let result = controller.execute(intent).await;
    
    assert!(result.is_err());
    let err = result.err().unwrap().to_string();
    println!("Caught expected security error: {}", err);
    
    assert!(err.contains("Security violation"));
}
