//! Admin API for Multiagent management dashboard.
//!
//! Provides endpoints for:
//! - LLM Provider management
//! - S3 Persistence configuration
//! - MCP Registry management
//! - Metrics and observability
//! - Audit log queries
//! - Static dashboard UI

use axum::{
    body::Body,
    extract::{Path, Query, State, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, delete},
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use multi_agent_governance::{AuditStore, AuditFilter, AuditEntry, RbacConnector};
use multi_agent_governance::privacy::{PrivacyController, DeletionReport};

/// Embedded static assets for the dashboard.
#[derive(RustEmbed)]
#[folder = "../../dashboard/static"]
struct Asset;

use multi_agent_skills::mcp_registry::{McpRegistry, McpServerInfo};
use multi_agent_governance::SecretsManager;
use multi_agent_core::traits::{ProviderStore, ArtifactStore, SessionStore};

pub mod doctor;

// =========================================
// State & Data Structures
// =========================================

/// Admin API state.
pub struct AdminState {
    pub audit_store: Arc<dyn AuditStore>,
    pub rbac: Arc<dyn RbacConnector>,
    pub metrics: Option<metrics_exporter_prometheus::PrometheusHandle>,
    pub mcp_registry: Arc<McpRegistry>,
    /// In-memory provider storage (used when `provider_store` is None).
    pub providers: Arc<RwLock<Vec<ProviderEntry>>>,
    /// External provider store (e.g., Redis/PostgreSQL).
    /// When set, this is used instead of in-memory `providers`.
    pub provider_store: Option<Arc<dyn ProviderStore>>,
    /// Secrets manager for encrypting sensitive data (API keys).
    pub secrets: Arc<dyn SecretsManager>,
    /// Privacy controller for GDPR operations.
    pub privacy_controller: Option<Arc<PrivacyController>>,
    /// Artifact Store for diagnostics.
    pub artifact_store: Option<Arc<dyn ArtifactStore>>,
    /// Session Store for diagnostics.
    pub session_store: Option<Arc<dyn SessionStore>>,
}


/// LLM Provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub id: String,
    pub vendor: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Key ID for retrieving the encrypted API key from SecretsManager.
    /// The actual API key is never stored in plain text.
    #[serde(skip_serializing)]
    pub api_key_id: String,
    pub capabilities: Vec<String>,
    pub status: String,
}


/// Request to add a provider.
#[derive(Debug, Deserialize)]
pub struct AddProviderRequest {
    pub vendor: String,
    pub model_id: String,
    pub description: Option<String>,
    pub base_url: String,
    pub version: Option<String>,
    pub api_key: String,
    pub capabilities: Vec<String>,
}

/// Request to test a provider connection.
#[derive(Debug, Deserialize)]
pub struct TestProviderRequest {
    pub base_url: String,
    pub api_key: String,
    pub model_id: String,
}

/// S3 Config request.
#[derive(Debug, Deserialize)]
pub struct S3ConfigRequest {
    pub bucket: String,
    pub endpoint: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    pub region: Option<String>,
}

/// MCP Server registration request.
#[derive(Debug, Deserialize)]
pub struct RegisterMcpRequest {
    pub name: String,
    pub transport_type: String,
    pub command: String,
    pub capabilities: Vec<String>,
}

/// Response for config endpoint.
#[derive(Serialize)]
pub struct ConfigResponse {
    pub version: String,
    pub features: Vec<String>,
    pub persistence: PersistenceConfig,
    pub llm: LlmConfig,
}

#[derive(Serialize)]
pub struct PersistenceConfig {
    pub mode: String,
    pub s3_bucket: Option<String>,
    pub s3_endpoint: Option<String>,
}

#[derive(Serialize)]
pub struct LlmConfig {
    pub provider_source: String,
    pub providers_file_present: bool,
}

/// Query parameters for audit endpoint.
#[derive(Deserialize)]
pub struct AuditQuery {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub limit: Option<usize>,
}

// =========================================
// Middleware
// =========================================

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

// =========================================
// Provider Endpoints
// =========================================

/// List all providers.
async fn list_providers(
    State(state): State<Arc<AdminState>>,
) -> Json<Vec<ProviderEntry>> {
    if let Some(store) = &state.provider_store {
        if let Ok(providers) = store.list().await {
            // Convert legacy core::ProviderEntry to admin::ProviderEntry
            let admin_providers = providers.into_iter().map(|p| ProviderEntry {
                id: p.id,
                vendor: p.vendor,
                model_id: p.model_id,
                description: p.description,
                base_url: p.base_url,
                version: p.version,
                api_key_id: p.api_key_id,
                capabilities: p.capabilities,
                status: p.status,
            }).collect();
            return Json(admin_providers);
        }
        tracing::error!("Failed to list providers from store");
        return Json(vec![]);
    }
    let providers = state.providers.read().await;
    Json(providers.clone())
}

/// Add a new provider.
async fn add_provider(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<AddProviderRequest>,
) -> Result<Json<ProviderEntry>, StatusCode> {
    let provider_id = format!("prov-{}", chrono::Utc::now().timestamp_millis());
    
    // Encrypt the API key and store it in the secrets manager
    let api_key_id = format!("api_key:{}", provider_id);
    state.secrets.store(&api_key_id, &req.api_key).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let entry = ProviderEntry {
        id: provider_id,
        vendor: req.vendor,
        model_id: req.model_id,
        description: req.description,
        base_url: req.base_url,
        version: req.version,
        api_key_id,
        capabilities: req.capabilities,
        status: "active".to_string(), // Set to active by default
    };

    if let Some(store) = &state.provider_store {
        // Convert to core::ProviderEntry
        let core_entry = multi_agent_core::traits::ProviderEntry {
            id: entry.id.clone(),
            vendor: entry.vendor.clone(),
            model_id: entry.model_id.clone(),
            description: entry.description.clone(),
            base_url: entry.base_url.clone(),
            version: entry.version.clone(),
            api_key_id: entry.api_key_id.clone(),
            capabilities: entry.capabilities.clone(),
            status: entry.status.clone(),
        };
        store.upsert(&core_entry).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        let mut providers = state.providers.write().await;
        providers.push(entry.clone());
    }
    
    // Log audit event
    let _ = state.audit_store.log(multi_agent_governance::AuditEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        user_id: "admin".to_string(),
        action: "ADD_PROVIDER".to_string(),
        resource: entry.id.clone(),
        outcome: multi_agent_governance::AuditOutcome::Success,
        metadata: Some(serde_json::json!({
            "vendor": entry.vendor,
            "model_id": entry.model_id
        })),
    }).await;


    Ok(Json(entry))
}


/// Test provider connection.
async fn test_provider(
    Json(req): Json<TestProviderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Simple connectivity check - try to reach the base URL
    let client = reqwest::Client::new();
    
    let result = client
        .get(&format!("{}/models", req.base_url))
        .header("Authorization", format!("Bearer {}", req.api_key))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match result {
        Ok(res) if res.status().is_success() || res.status() == 401 => {
            // 401 is acceptable - means server responded
            Ok(Json(serde_json::json!({"status": "connected"})))
        }
        _ => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

/// Test a specific provider by ID.
async fn test_provider_by_id(
    State(state): State<Arc<AdminState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut providers = state.providers.write().await;
    
    if let Some(provider) = providers.iter_mut().find(|p| p.id == id) {
        // Decrypt the API key from secrets manager
        let api_key = state.secrets.retrieve(&provider.api_key_id).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
        
        let client = reqwest::Client::new();
        
        let result = client
            .get(&format!("{}/models", provider.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;

        match result {
            Ok(res) if res.status().is_success() || res.status() == 401 => {
                provider.status = "connected".to_string();
                Ok(Json(serde_json::json!({"status": "connected"})))
            }
            _ => {
                provider.status = "error".to_string();
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}


/// Delete a provider.
async fn delete_provider(
    State(state): State<Arc<AdminState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut deleted = false;
    let mut api_key_id = None;

    if let Some(store) = &state.provider_store {
        // First get the provider to find the api_key_id
        if let Ok(Some(provider)) = store.get(&id).await {
            api_key_id = Some(provider.api_key_id.clone());
            if let Ok(result) = store.delete(&id).await {
                deleted = result;
            }
        }
    } else {
        let mut providers = state.providers.write().await;
        if let Some(pos) = providers.iter().position(|p| p.id == id) {
            api_key_id = Some(providers[pos].api_key_id.clone());
            providers.remove(pos);
            deleted = true;
        }
    }

    if deleted {
        // Also cleanup the secret
        if let Some(key_id) = api_key_id {
            let _ = state.secrets.delete(&key_id).await;
        }
        
        let _ = state.audit_store.log(multi_agent_governance::AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_id: "admin".to_string(),
            action: "DELETE_PROVIDER".to_string(),
            resource: id,
            outcome: multi_agent_governance::AuditOutcome::Success,
            metadata: None,
        }).await;

        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}


// =========================================
// Persistence Endpoints
// =========================================

/// Test S3 connection.
async fn test_s3_connection(
    Json(req): Json<S3ConfigRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use aws_config::Region;
    use aws_sdk_s3::config::{Credentials, Builder as S3ConfigBuilder};

    let creds = Credentials::new(
        &req.access_key,
        &req.secret_key,
        None,
        None,
        "admin-test"
    );

    let mut config_builder = S3ConfigBuilder::new()
        .credentials_provider(creds)
        .region(Region::new(req.region.unwrap_or_else(|| "us-east-1".to_string())))
        .behavior_version_latest();

    if let Some(endpoint) = req.endpoint {
        config_builder = config_builder.endpoint_url(endpoint).force_path_style(true);
    }

    let s3_config = config_builder.build();
    let client = aws_sdk_s3::Client::from_conf(s3_config);

    match client.head_bucket().bucket(&req.bucket).send().await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "connected"}))),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

// =========================================
// MCP Endpoints
// =========================================

/// Get MCP servers.
async fn get_mcp_servers(
    State(state): State<Arc<AdminState>>,
) -> Json<Vec<McpServerInfo>> {
    Json(state.mcp_registry.list_all())
}

/// Register MCP server.
async fn register_mcp(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<RegisterMcpRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use multi_agent_skills::mcp_registry::McpCapability;
    
    // Map string capabilities to enum
    let capabilities: Vec<McpCapability> = req.capabilities.iter().filter_map(|s| {
        match s.to_lowercase().as_str() {
            "tools" | "filesystem" => Some(McpCapability::FileSystem),
            "resources" | "database" => Some(McpCapability::Database),
            "prompts" | "web" => Some(McpCapability::Web),
            "code" | "code_execution" => Some(McpCapability::CodeExecution),
            "search" => Some(McpCapability::Search),
            "memory" => Some(McpCapability::Memory),
            "git" => Some(McpCapability::Git),
            "communication" => Some(McpCapability::Communication),
            other => Some(McpCapability::Custom(other.to_string())),
        }
    }).collect();

    
    let info = McpServerInfo {
        id: format!("mcp-{}", chrono::Utc::now().timestamp_millis()),
        name: req.name.clone(),
        description: format!("Registered via Admin UI: {}", req.name),
        capabilities,
        keywords: vec![req.name.clone()],
        connection_uri: req.command,
        args: vec![],
        transport_type: req.transport_type,
        priority: 50,
        available: true,
    };

    state.mcp_registry.register(info.clone());
    Ok(Json(serde_json::json!({"id": info.id, "status": "registered"})))
}


/// Remove MCP server.
async fn remove_mcp(
    State(state): State<Arc<AdminState>>,
    Path(id): Path<String>,
) -> StatusCode {
    state.mcp_registry.unregister(&id);
    StatusCode::NO_CONTENT
}

// =========================================
// Config & Health Endpoints
// =========================================

/// Health check endpoint (public).
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

/// Get current configuration.
async fn get_config() -> impl IntoResponse {
    let (p_mode, p_bucket, p_endpoint) = if let Ok(bucket) = std::env::var("AWS_S3_BUCKET") {
        ("S3 (Tiered)".to_string(), Some(bucket), std::env::var("AWS_ENDPOINT_URL").ok())
    } else {
        ("In-Memory".to_string(), None, None)
    };

    let providers_path = std::path::Path::new("providers.json");
    let has_providers = providers_path.exists();
    let source = if has_providers { "File (providers.json)" } else { "Environment Variables" };

    Json(ConfigResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: vec![
            "rbac".to_string(),
            "audit".to_string(),
            "secrets_encryption".to_string(),
            "mcp".to_string(),
            "providers_api".to_string(),
        ],
        persistence: PersistenceConfig {
            mode: p_mode,
            s3_bucket: p_bucket,
            s3_endpoint: p_endpoint,
        },
        llm: LlmConfig {
            provider_source: source.to_string(),
            providers_file_present: has_providers,
        },
    })
}

// =========================================
// Audit & Metrics Endpoints
// =========================================

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
            (latency_sum / latency_count as f64) * 1000.0
        } else {
            0.0
        };

        Json(serde_json::json!({
            "requests_total": requests_total,
            "tokens_used": tokens_used,
            "active_sessions": 0,
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

// =========================================
// Static File Handlers
// =========================================

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

// =========================================
// Router
// =========================================

/// Build the admin API router.
pub fn admin_router(state: Arc<AdminState>) -> Router {
    let api_routes = Router::new()
        // Config & Metrics
        .route("/config", get(get_config))
        .route("/metrics", get(get_metrics))
        .route("/audit", get(get_audit))
        // Providers
        .route("/providers", get(list_providers).post(add_provider))
        .route("/providers/test", post(test_provider))
        .route("/providers/:id", delete(delete_provider))
        .route("/providers/:id/test", post(test_provider_by_id))
        // Persistence
        .route("/persistence/test", post(test_s3_connection))
        // Governance
        .route("/governance/forget-user", post(forget_user))
        // Doctor
        .route("/doctor", post(doctor::check_all))
        // MCP
        .route("/mcp/servers", get(get_mcp_servers))
        .route("/mcp/register", post(register_mcp))
        .route("/mcp/servers/:id", delete(remove_mcp))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .merge(api_routes)
        .route("/", get(index_handler))
        .route("/health", get(health))
        .route("/*path", get(static_handler))
        .with_state(state)
}
