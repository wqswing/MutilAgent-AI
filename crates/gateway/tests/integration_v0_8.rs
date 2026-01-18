use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt; // for oneshot
use std::sync::Arc;
use multi_agent_gateway::{GatewayServer, GatewayConfig, DefaultRouter, InMemorySemanticCache};
use multi_agent_admin::AdminState;
use multi_agent_governance::{FileAuditStore, AesGcmSecretsManager, NoOpRbacConnector, setup_metrics_recorder};

#[tokio::test]
async fn test_v0_8_features_integration() {
    // 1. Setup Environment
    // Use a unique file for audit logic to avoid collision
    let audit_file = "test_audit_v0_8.log";
    let _ = std::fs::remove_file(audit_file); // Clean up previous run

    // Initialize Governance
    let audit_store = Arc::new(FileAuditStore::new(audit_file));
    let rbac = Arc::new(NoOpRbacConnector);
    // Metrics setup might fail if already initialized in another test, so handle error gracefully or assume it works once globally
    let metrics_handle = setup_metrics_recorder().ok(); 

    let admin_state = Arc::new(AdminState {
        audit_store: audit_store.clone(),
        rbac: rbac.clone(),
        metrics: metrics_handle.clone(),
        mcp_registry: Arc::new(multi_agent_skills::McpRegistry::new()),
        providers: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        secrets: Arc::new(multi_agent_governance::AesGcmSecretsManager::new(None)),
    });

    // Initialize Gateway
    let config = GatewayConfig {
        host: "127.0.0.1".to_string(),
        port: 0, // Random port
        enable_cors: false,
        enable_tracing: false,
    };
    
    // Mocks for Gateway deps
    let router = Arc::new(DefaultRouter::new());
    let llm_client = Arc::new(multi_agent_model_gateway::MockLlmClient::new("dummy"));
    let cache = Arc::new(InMemorySemanticCache::new(llm_client));

    let server = GatewayServer::new(config, router, cache)
        .with_admin(admin_state)
        .with_metrics(metrics_handle.expect("Metrics handler must be available for this test"));

    let app = server.build_router();

    // 2. Test Cases

    // Case A: Public Health Check
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/health")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Case B: Admin Config (Unauthorized - No Token)
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/admin/config")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Case C: Admin Config (Forbidden - User Token)
    // NoOpRbacConnector returns "user" role for default token
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/admin/config")
            .header("Authorization", "Bearer somerandomtoken")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Case D: Admin Config (Authorized - Admin Token)
    // NoOpRbacConnector returns "admin" role for "admin" token
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/admin/config")
            .header("Authorization", "Bearer admin")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Case E: Metrics Endpoint (Authorized)
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/admin/metrics")
            .header("Authorization", "Bearer admin")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    // We can't easily check the body string in oneshot without reading the stream, 
    // but OK status implies authentication worked.

    // Case F: Verify Audit Log persistence
    // We need to trigger an audit event first. 
    // Currently Admin API reads logs, but maybe modifying config writes logs?
    // Actually, the current Admin API implementation is read-only for now, 
    // but let's verify we can Write to the audit store manually and Read it back via API.
    
    use multi_agent_governance::{AuditStore, AuditEntry};
    let entry = AuditEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        user_id: "test_admin".to_string(),
        action: "TEST_ACTION".to_string(),
        resource: "test_resource".to_string(),
        outcome: multi_agent_governance::AuditOutcome::Success,
        metadata: Some(serde_json::json!({"foo": "bar"})),
    };
    audit_store.log(entry).await.unwrap();

    // Now query via API
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/admin/audit?action=TEST_ACTION")
            .header("Authorization", "Bearer admin")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // 3. Cleanup
    let _ = std::fs::remove_file(audit_file);
}
