use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use multi_agent_core::mocks::{MockRouter, MockSemanticCache};
use multi_agent_core::traits::Controller;
use multi_agent_core::types::{AgentResult, UserIntent};
use multi_agent_gateway::{GatewayConfig, GatewayServer};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

struct MockController;

#[async_trait]
impl Controller for MockController {
    async fn execute(
        &self,
        _intent: UserIntent,
        _trace_id: String,
    ) -> multi_agent_core::Result<AgentResult> {
        Ok(AgentResult::Text("Mock response".to_string()))
    }
    async fn resume(
        &self,
        _session_id: &str,
        _user_id: Option<&str>,
    ) -> multi_agent_core::Result<AgentResult> {
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
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
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
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .body(Body::from(json!({"message": "test message"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["version"], "v1");
    assert_eq!(json["data"]["intent"]["type"], "complex_mission");
}

#[tokio::test]
async fn test_chat_endpoint_with_controller() {
    let config = GatewayConfig::default();
    let router = Arc::new(MockRouter::complex_mission("test goal"));
    let cache = Arc::new(MockSemanticCache::new());
    let controller = Arc::new(MockController);

    let server = GatewayServer::new(config, router, cache).with_controller(controller);
    let app = server.build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat")
                .header("Content-Type", "application/json")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .body(Body::from(json!({"message": "hello"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["version"], "v1");
    assert_eq!(json["data"]["result"]["type"], "Text");
    assert_eq!(json["data"]["result"]["payload"], "Mock response");
}

#[tokio::test]
async fn test_gateway_schema_endpoint() {
    let config = GatewayConfig::default();
    let router = Arc::new(MockRouter::complex_mission("test"));
    let cache = Arc::new(MockSemanticCache::new());
    let server = GatewayServer::new(config, router, cache);
    let app = server.build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/system/schema/gateway")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "gateway_contract");
    assert_eq!(json["version"], "v1");
    assert!(json["schema"]["$schema"].is_string());
}

#[tokio::test]
async fn test_webhook_idempotency_replay_and_conflict() {
    let config = GatewayConfig::default();
    let router = Arc::new(MockRouter::complex_mission("test"));
    let cache = Arc::new(MockSemanticCache::new());
    let server = GatewayServer::new(config, router, cache);
    let app = server.build_router();

    let key = "idem-webhook-1";
    let body = json!({"event":"user.created","id":"u-1"}).to_string();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/webhook/user_created")
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", key)
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_status = first.status();
    let first_body = axum::body::to_bytes(first.into_body(), usize::MAX)
        .await
        .unwrap();

    let replay = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/webhook/user_created")
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", key)
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), first_status);
    let replay_body = axum::body::to_bytes(replay.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(first_body, replay_body);

    let conflict = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/webhook/user_created")
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", key)
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .body(Body::from(
                    json!({"event":"user.deleted","id":"u-1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
}
