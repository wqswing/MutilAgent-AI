use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use serde_json::{json, Value};
use std::sync::Arc;
use multi_agent_gateway::{GatewayServer, GatewayConfig};
use multi_agent_core::mocks::{MockRouter, MockSemanticCache};
use multi_agent_core::types::{UserIntent, AgentResult};
use multi_agent_core::traits::Controller;
use async_trait::async_trait;

struct MockController;

#[async_trait]
impl Controller for MockController {
    async fn execute(&self, _intent: UserIntent) -> multi_agent_core::Result<AgentResult> {
        Ok(AgentResult::Text("Mock response".to_string()))
    }
    async fn resume(&self, _session_id: &str) -> multi_agent_core::Result<AgentResult> {
        Ok(AgentResult::Text("Resumed".to_string()))
    }
    async fn cancel(&self, _session_id: &str) -> multi_agent_core::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_health_endpoint() {
    let config = GatewayConfig::default();
    let router = Arc::new(MockRouter::complex_mission("test"));
    let cache = Arc::new(MockSemanticCache::new());
    let server = GatewayServer::new(config, router, cache);
    let app = server.build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_intent_endpoint() {
    let config = GatewayConfig::default();
    let router = Arc::new(MockRouter::complex_mission("find a place"));
    let cache = Arc::new(MockSemanticCache::new());
    let server = GatewayServer::new(config, router, cache);
    let app = server.build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/intent")
                .header("Content-Type", "application/json")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
                .body(Body::from(json!({"message": "test message"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["intent"]["type"], "complex_mission");
}

#[tokio::test]
async fn test_chat_endpoint_with_controller() {
    let config = GatewayConfig::default();
    let router = Arc::new(MockRouter::complex_mission("test goal"));
    let cache = Arc::new(MockSemanticCache::new());
    let controller = Arc::new(MockController);
    
    let server = GatewayServer::new(config, router, cache)
        .with_controller(controller);
    let app = server.build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat")
                .header("Content-Type", "application/json")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
                .body(Body::from(json!({"message": "hello"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["result"]["type"], "Text");
    assert_eq!(json["result"]["payload"], "Mock response");
}

