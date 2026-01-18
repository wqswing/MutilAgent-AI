//! Admin API for Multiagent management dashboard.
//!
//! Provides endpoints for:
//! - Configuration management
//! - Metrics and observability
//! - Audit log queries
//! - Static dashboard UI

use axum::{
    body::Body,
    extract::{Path, Query, State, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use multi_agent_governance::{AuditStore, AuditFilter, AuditEntry, RbacConnector};

/// Embedded static assets for the dashboard.
#[derive(RustEmbed)]
#[folder = "../../dashboard/static"]
struct Asset;

/// Admin API state.
pub struct AdminState {
    pub audit_store: Arc<dyn AuditStore>,
    pub rbac: Arc<dyn RbacConnector>,
    pub metrics: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

// ... existing structs ...

// ... existing structs ...

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

/// Authentication middleware.
async fn auth_middleware(
    State(state): State<Arc<AdminState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    
    let auth_header = req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match auth_header {
        Some(token) => {
            match state.rbac.validate(token).await {
                Ok(roles) => {
                     // Check if admin
                     if roles.is_admin {
                         Ok(next.run(req).await)
                     } else {
                         Err(StatusCode::FORBIDDEN)
                     }
                }
                Err(_) => Err(StatusCode::UNAUTHORIZED),
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Health check endpoint (public).
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

/// Get metrics.
async fn get_metrics(
    State(state): State<Arc<AdminState>>,
) -> impl IntoResponse {
    if let Some(handle) = &state.metrics {
        // Return raw prometheus text for now, or parsed JSON if desired.
        // For Dashboard compatibility, we need JSON.
        // We'll parse the text output simply to extract key metrics.
        let output = handle.render();
        
        let mut requests_total = 0;
        let mut tokens_used = 0;
        let mut latency_sum = 0.0;
        let mut latency_count = 0;
        
        for line in output.lines() {
            if line.starts_with("http_requests_total") {
                if let Some(val) = line.split_whitespace().last().and_then(|v| v.parse::<u64>().ok()) {
                    requests_total += val;
                }
            } else if line.starts_with("llm_token_usage_total") {
                 if let Some(val) = line.split_whitespace().last().and_then(|v| v.parse::<u64>().ok()) {
                    tokens_used += val;
                }
            } else if line.starts_with("http_request_duration_seconds_sum") {
                if let Some(val) = line.split_whitespace().last().and_then(|v| v.parse::<f64>().ok()) {
                    latency_sum += val;
                }
            } else if line.starts_with("http_request_duration_seconds_count") {
                if let Some(val) = line.split_whitespace().last().and_then(|v| v.parse::<u64>().ok()) {
                    latency_count += val;
                }
            }
        }
        
        let avg_latency = if latency_count > 0 {
            (latency_sum / latency_count as f64) * 1000.0 // ms
        } else {
            0.0
        };

        Json(serde_json::json!({
            "requests_total": requests_total,
            "tokens_used": tokens_used,
            "active_sessions": 0, // Not tracked yet
            "avg_latency_ms": avg_latency
        }))
    } else {
         Json(serde_json::json!({
            "requests_total": 0,
            "tokens_used": 0,
            "active_sessions": 0
        }))
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
    let api_routes = Router::new()
        .route("/config", get(get_config))
        .route("/metrics", get(get_metrics))
        .route("/audit", get(get_audit))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .merge(api_routes)
        .route("/", get(index_handler))
        .route("/health", get(health)) // Public health check
        .route("/*path", get(static_handler)) // Static files
        .with_state(state)
}

