//! Admin API for Multiagent management dashboard.
//!
//! Provides endpoints for:
//! - Configuration management
//! - Metrics and observability
//! - Audit log queries

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use multi_agent_governance::{AuditStore, AuditFilter, AuditEntry};

/// Admin API state.
pub struct AdminState {
    pub audit_store: Arc<dyn AuditStore>,
}

/// Response for config endpoint.
#[derive(Serialize)]
pub struct ConfigResponse {
    pub version: String,
    pub features: Vec<String>,
}

/// Query parameters for audit endpoint.
#[derive(Deserialize)]
pub struct AuditQuery {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub limit: Option<usize>,
}

/// Health check endpoint.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

/// Get current configuration.
async fn get_config() -> impl IntoResponse {
    Json(ConfigResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: vec![
            "rbac".to_string(),
            "audit".to_string(),
            "secrets_encryption".to_string(),
        ],
    })
}

/// Get metrics (placeholder).
async fn get_metrics() -> impl IntoResponse {
    Json(serde_json::json!({
        "requests_total": 0,
        "tokens_used": 0,
        "active_sessions": 0
    }))
}

/// Query audit logs.
async fn get_audit(
    State(state): State<Arc<AdminState>>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, StatusCode> {
    let filter = AuditFilter {
        user_id: query.user_id,
        action: query.action,
        limit: query.limit,
        ..Default::default()
    };
    
    match state.audit_store.query(filter).await {
        Ok(entries) => Ok(Json(entries)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Build the admin API router.
pub fn admin_router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/config", get(get_config))
        .route("/metrics", get(get_metrics))
        .route("/audit", get(get_audit))
        .with_state(state)
}
