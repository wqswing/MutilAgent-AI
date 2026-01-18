//! Admin API for Multiagent management dashboard.
//!
//! Provides endpoints for:
//! - Configuration management
//! - Metrics and observability
//! - Audit log queries
//! - Static dashboard UI

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use multi_agent_governance::{AuditStore, AuditFilter, AuditEntry};

/// Embedded static assets for the dashboard.
#[derive(RustEmbed)]
#[folder = "../../dashboard/static"]
struct Asset;

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

/// Serve static files from embedded assets.
async fn static_handler(Path(path): Path<String>) -> impl IntoResponse {
    let path = if path.is_empty() { "index.html".to_string() } else { path };
    
    match Asset::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap(),
    }
}

/// Serve index.html for root path.
async fn index_handler() -> impl IntoResponse {
    match Asset::get("index.html") {
        Some(content) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content.data.to_vec()))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Dashboard not found"))
            .unwrap(),
    }
}

/// Build the admin API router.
pub fn admin_router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/health", get(health))
        .route("/config", get(get_config))
        .route("/metrics", get(get_metrics))
        .route("/audit", get(get_audit))
        .route("/*path", get(static_handler))
        .with_state(state)
}
