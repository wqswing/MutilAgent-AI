//! Integration tests for RBAC and Audit modules.

use multi_agent_governance::{
    AuditEntry, AuditFilter, AuditOutcome, AuditStore, InMemoryAuditStore, NoOpRbacConnector,
    RbacConnector,
};

#[tokio::test]
async fn test_rbac_noop_connector_allows_all() {
    let connector = NoOpRbacConnector;

    // Validate returns anonymous user
    let roles = connector.validate("any_token").await.unwrap();
    assert_eq!(roles.user_id, "anonymous");
    assert!(!roles.is_admin);

    // Check permission always returns true
    let allowed = connector
        .check_permission("token", "any_resource", "any_action")
        .await
        .unwrap();
    assert!(allowed);
}

#[tokio::test]
async fn test_audit_store_log_and_query() {
    let store = InMemoryAuditStore::new();

    // Log entries
    store
        .log(AuditEntry {
            id: "1".to_string(),
            timestamp: "2026-01-18T12:00:00Z".to_string(),
            user_id: "admin".to_string(),
            action: "update_config".to_string(),
            resource: "llm_key".to_string(),
            outcome: AuditOutcome::Success,
            metadata: None,
            previous_hash: None,
            hash: None,
        })
        .await
        .unwrap();

    store
        .log(AuditEntry {
            id: "2".to_string(),
            timestamp: "2026-01-18T12:01:00Z".to_string(),
            user_id: "user1".to_string(),
            action: "execute_tool".to_string(),
            resource: "calculator".to_string(),
            outcome: AuditOutcome::Success,
            metadata: None,
            previous_hash: None,
            hash: None,
        })
        .await
        .unwrap();

    store
        .log(AuditEntry {
            id: "3".to_string(),
            timestamp: "2026-01-18T12:02:00Z".to_string(),
            user_id: "user1".to_string(),
            action: "execute_tool".to_string(),
            resource: "dangerous_tool".to_string(),
            outcome: AuditOutcome::Denied,
            metadata: None,
            previous_hash: None,
            hash: None,
        })
        .await
        .unwrap();

    // Query all
    let all = store.query(AuditFilter::default()).await.unwrap();
    assert_eq!(all.len(), 3);

    // Query by user
    let admin_entries = store
        .query(AuditFilter {
            user_id: Some("admin".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(admin_entries.len(), 1);
    assert_eq!(admin_entries[0].action, "update_config");

    // Query by action
    let tool_entries = store
        .query(AuditFilter {
            action: Some("execute_tool".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(tool_entries.len(), 2);

    // Query with limit
    let limited = store
        .query(AuditFilter {
            limit: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(limited.len(), 2);
}

#[tokio::test]
async fn test_audit_outcome_serialization() {
    let entry = AuditEntry {
        id: "test".to_string(),
        timestamp: "2026-01-18T12:00:00Z".to_string(),
        user_id: "test_user".to_string(),
        action: "test_action".to_string(),
        resource: "test_resource".to_string(),
        outcome: AuditOutcome::Error("Connection failed".to_string()),
        metadata: Some(serde_json::json!({"retry_count": 3})),
        previous_hash: None,
        hash: None,
    };

    // Serialize to JSON
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("test_user"));
    assert!(json.contains("Connection failed"));

    // Deserialize back
    let parsed: AuditEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "test");
    assert_eq!(parsed.user_id, "test_user");
}
