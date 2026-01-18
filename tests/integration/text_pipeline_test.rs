//! Integration test for the text pipeline.
//!
//! Tests the flow: L0 Gateway -> L1 Controller -> L2 Skills

use std::sync::Arc;

use multiagent_controller::ReActController;
use multiagent_controller::react::ReActConfig;
use multiagent_core::traits::{Controller, IntentRouter, SemanticCache, ToolRegistry};
use multiagent_core::types::{NormalizedRequest, UserIntent};
use multiagent_gateway::{DefaultRouter, InMemorySemanticCache};
use multiagent_skills::{DefaultToolRegistry, EchoTool, CalculatorTool};
use multiagent_store::InMemoryStore;

#[tokio::test]
async fn test_full_text_pipeline() {
    // Initialize components
    let store = Arc::new(InMemoryStore::new());
    let tools = Arc::new(DefaultToolRegistry::new());
    
    tools.register(Box::new(EchoTool)).await.unwrap();
    tools.register(Box::new(CalculatorTool)).await.unwrap();

    let router = DefaultRouter::new();
    let cache = InMemorySemanticCache::new();
    let controller = ReActController::new(ReActConfig::default())
        .with_store(store.clone());

    // Test 1: Simple query -> should be classified as FastAction or ComplexMission
    let request = NormalizedRequest::text("Help me write a function in Rust");
    let intent = router.classify(&request).await.unwrap();

    match &intent {
        UserIntent::ComplexMission { goal, .. } => {
            println!("Classified as ComplexMission: {}", goal);
            assert!(goal.len() > 0);
        }
        UserIntent::FastAction { tool_name, .. } => {
            println!("Classified as FastAction: {}", tool_name);
        }
    }

    // Test 2: Execute the intent through controller
    let result = controller.execute(intent).await.unwrap();
    println!("Controller result: {:?}", result);

    // Test 3: Semantic cache
    cache.set("What is Rust?", "Rust is a systems programming language.").await.unwrap();
    let cached = cache.get("What is Rust?").await.unwrap();
    assert!(cached.is_some());
    assert!(cached.unwrap().contains("systems programming"));

    // Test 4: Cache miss
    let miss = cache.get("What is Python?").await.unwrap();
    assert!(miss.is_none());

    // Test 5: Tool execution
    let echo_result = tools.execute("echo", serde_json::json!({"message": "Hello"})).await.unwrap();
    assert!(echo_result.success);
    assert!(echo_result.content.contains("Hello"));

    let calc_result = tools.execute("calculator", serde_json::json!({
        "operation": "add",
        "a": 5,
        "b": 3
    })).await.unwrap();
    assert!(calc_result.success);
    assert!(calc_result.data.unwrap()["result"] == 8.0);

    println!("All pipeline tests passed!");
}

#[tokio::test]
async fn test_artifact_store_pass_by_reference() {
    use bytes::Bytes;
    use multiagent_core::traits::ArtifactStore;
    use multiagent_store::{maybe_store_by_ref, LARGE_CONTENT_THRESHOLD};

    let store = Arc::new(InMemoryStore::new());

    // Small content - should not be stored by reference
    let small_content = "Hello, World!".to_string();
    let (result, ref_id) = maybe_store_by_ref(store.as_ref(), small_content.clone()).await.unwrap();
    assert_eq!(result, small_content);
    assert!(ref_id.is_none());

    // Large content - should be stored by reference
    let large_content = "x".repeat(LARGE_CONTENT_THRESHOLD + 100);
    let (result, ref_id) = maybe_store_by_ref(store.as_ref(), large_content.clone()).await.unwrap();
    assert!(result.contains("RefID"));
    assert!(ref_id.is_some());

    // Verify we can retrieve the content
    let retrieved = store.load(&ref_id.unwrap()).await.unwrap().unwrap();
    assert_eq!(String::from_utf8_lossy(&retrieved), large_content);
}

#[tokio::test]
async fn test_intent_routing() {
    let router = DefaultRouter::new();

    // Complex mission
    let request = NormalizedRequest::text("Create a REST API");
    let intent = router.classify(&request).await.unwrap();
    assert!(matches!(intent, UserIntent::ComplexMission { .. }));

    // Fast action
    let request = NormalizedRequest::text("Search for Rust tutorials");
    let intent = router.classify(&request).await.unwrap();
    assert!(matches!(intent, UserIntent::FastAction { .. }));
}

#[tokio::test]
async fn test_controller_fast_path() {
    let tools = Arc::new(DefaultToolRegistry::new());
    tools.register(Box::new(EchoTool)).await.unwrap();

    let controller = ReActController::new(ReActConfig::default());

    let intent = UserIntent::FastAction {
        tool_name: "test_tool".to_string(),
        args: serde_json::json!({"query": "test"}),
    };

    let result = controller.execute(intent).await.unwrap();
    // Should return mock response since tools not wired up
    match result {
        multiagent_core::types::AgentResult::Text(text) => {
            assert!(text.contains("Fast path"));
        }
        _ => panic!("Expected Text result"),
    }
}
