use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use serde_json::{json, Value};
use std::sync::Arc;
use multi_agent_admin::AdminState;
use multi_agent_governance::{InMemoryAuditStore, NoOpRbacConnector, AesGcmSecretsManager, SecretsManager};
use multi_agent_skills::McpRegistry;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_admin_provider_crud_with_encryption() {
    let audit_store = Arc::new(InMemoryAuditStore::new());
    let rbac = Arc::new(NoOpRbacConnector);
    let mcp_registry = Arc::new(McpRegistry::new());
    let secrets = Arc::new(AesGcmSecretsManager::new(None));
    let providers = Arc::new(RwLock::new(Vec::new()));

    let state = Arc::new(AdminState {
        audit_store,
        rbac,
        metrics: None,
        mcp_registry,
        providers,
        secrets: secrets.clone(),
    });

    let app = multi_agent_admin::admin_router(state);

    // 1. Add a provider
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/providers")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer admin")
                .body(Body::from(json!({
                    "vendor": "openai",
                    "model_id": "gpt-4",
                    "base_url": "https://api.openai.com/v1",
                    "api_key": "sk-test-key",
                    "capabilities": ["text"]
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let provider_id = json["id"].as_str().expect("Provider ID not found").to_string();
    let api_key_id = format!("api_key:{}", provider_id);
    
    // api_key should NOT be in the response
    assert!(json["api_key"].is_null());

    // 2. Verify key is encrypted in secrets manager
    let retrieved_key = secrets.retrieve(&api_key_id).await.unwrap();
    assert_eq!(retrieved_key, Some("sk-test-key".to_string()));

    // 3. List providers
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/providers")
            .header("Authorization", "Bearer admin")
            .body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let list: Value = serde_json::from_slice(&body).unwrap();
    assert!(list.as_array().unwrap().len() > 0);

    // 4. Delete provider
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(&format!("/providers/{}", provider_id))
                .header("Authorization", "Bearer admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // 5. Verify secret is deleted
    let retrieved_key_after_delete = secrets.retrieve(&api_key_id).await.unwrap();
    assert!(retrieved_key_after_delete.is_none());
}
