//! Axum-based HTTP server for the gateway.

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use multi_agent_core::{
    traits::{Controller, IntentRouter, SemanticCache},
    types::{AgentResult, NormalizedRequest, RequestContent, RequestMetadata, UserIntent},
    Result,
};

/// Gateway configuration.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Host to bind to.
    pub host: String,
    /// Port to bind to.
    pub port: u16,
    /// Enable CORS.
    pub enable_cors: bool,
    /// Enable request tracing.
    pub enable_tracing: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            enable_cors: true,
            enable_tracing: true,
        }
    }
}

/// Shared application state.
pub struct AppState {
    /// Intent router.
    pub router: Arc<dyn IntentRouter>,
    /// Semantic cache.
    pub cache: Arc<dyn SemanticCache>,
    /// Controller (optional for Phase 1).
    pub controller: Option<Arc<dyn Controller>>,
}

use metrics_exporter_prometheus::PrometheusHandle;

/// Gateway server.
pub struct GatewayServer {
    config: GatewayConfig,
    state: Arc<AppState>,
    metrics_handle: Option<PrometheusHandle>,
}

impl GatewayServer {
    /// Create a new gateway server.
    pub fn new(
        config: GatewayConfig,
        router: Arc<dyn IntentRouter>,
        cache: Arc<dyn SemanticCache>,
    ) -> Self {
        Self {
            config,
            state: Arc::new(AppState {
                router,
                cache,
                controller: None,
            }),
            metrics_handle: None,
        }
    }

    /// Set the controller.
    pub fn with_controller(mut self, controller: Arc<dyn Controller>) -> Self {
        Arc::get_mut(&mut self.state).unwrap().controller = Some(controller);
        self
    }

    /// Set metrics handle.
    pub fn with_metrics(mut self, handle: PrometheusHandle) -> Self {
        self.metrics_handle = Some(handle);
        self
    }

    /// Build the Axum router.
    pub fn build_router(&self) -> Router {
        let mut router = Router::new()
            .route("/health", get(health_handler))
            .route("/v1/chat", post(chat_handler))
            .route("/v1/intent", post(intent_handler))
            .route("/v1/webhook/{event_type}", post(webhook_handler))
            .with_state(self.state.clone());

        if let Some(handle) = &self.metrics_handle {
            let handle = handle.clone();
            router = router.route("/metrics", get(move || async move { handle.render() }));
        }

        if self.config.enable_cors {
            router = router.layer(CorsLayer::new().allow_origin(Any).allow_methods(Any));
        }

        if self.config.enable_tracing {
            router = router.layer(TraceLayer::new_for_http());
        }

        router
    }

    /// Run the server.
    pub async fn run(self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| multi_agent_core::Error::gateway(format!("Failed to bind: {}", e)))?;

        tracing::info!(addr = %addr, "Gateway server starting");

        axum::serve(listener, self.build_router())
            .await
            .map_err(|e| multi_agent_core::Error::gateway(format!("Server error: {}", e)))?;

        Ok(())
    }
}

// =============================================================================
// Request/Response Types
// =============================================================================

/// Chat request.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// Message content.
    pub message: String,
    /// Optional session ID.
    pub session_id: Option<String>,
    /// Optional user ID.
    pub user_id: Option<String>,
}

/// Chat response.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    /// Trace ID for this request.
    pub trace_id: String,
    /// Classified intent.
    pub intent: UserIntent,
    /// Result (if controller is available).
    pub result: Option<AgentResult>,
    /// Whether the response was from cache.
    pub cached: bool,
}

/// Intent-only request.
#[derive(Debug, Deserialize)]
pub struct IntentRequest {
    /// Message to classify.
    pub message: String,
}

/// Intent response.
#[derive(Debug, Serialize)]
pub struct IntentResponse {
    /// Trace ID.
    pub trace_id: String,
    /// Classified intent.
    pub intent: UserIntent,
}

/// Health response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Status.
    pub status: String,
    /// Version.
    pub version: String,
}

/// Error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
    /// Trace ID.
    pub trace_id: Option<String>,
}

// =============================================================================
// Handlers
// =============================================================================

/// Health check handler.
async fn health_handler() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Chat handler.
async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> impl IntoResponse {
    let trace_id = Uuid::new_v4().to_string();

    tracing::info!(
        trace_id = %trace_id,
        message_len = payload.message.len(),
        "Processing chat request"
    );

    // Check semantic cache first
    match state.cache.get(&payload.message).await {
        Ok(Some(cached_response)) => {
            tracing::info!(trace_id = %trace_id, "Cache hit");
            return (
                StatusCode::OK,
                Json(ChatResponse {
                    trace_id,
                    intent: UserIntent::FastAction {
                        tool_name: "cache".to_string(),
                        args: serde_json::json!({}),
                    },
                    result: Some(AgentResult::Text(cached_response)),
                    cached: true,
                }),
            );
        }
        Ok(None) => {
            tracing::debug!(trace_id = %trace_id, "Cache miss");
        }
        Err(e) => {
            tracing::warn!(trace_id = %trace_id, error = %e, "Cache error");
        }
    }

    // Create normalized request
    let request = NormalizedRequest {
        trace_id: trace_id.clone(),
        content: payload.message.clone(),
        original_content: multi_agent_core::types::RequestContent::Text(payload.message.clone()),
        refs: Vec::new(),
        metadata: RequestMetadata {
            user_id: payload.user_id,
            session_id: payload.session_id,
            custom: Default::default(),
        },
    };

    // Classify intent
    let intent = match state.router.classify(&request).await {
        Ok(intent) => intent,
        Err(e) => {
            tracing::error!(trace_id = %trace_id, error = %e, "Failed to classify intent");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatResponse {
                    trace_id,
                    intent: UserIntent::ComplexMission {
                        goal: "Error".to_string(),
                        context_summary: e.to_string(),
                        visual_refs: Vec::new(),
                    },
                    result: Some(AgentResult::Error {
                        message: e.to_string(),
                        code: "ROUTING_ERROR".to_string(),
                    }),
                    cached: false,
                }),
            );
        }
    };

    // Execute via controller if available
    let result = if let Some(ref controller) = state.controller {
        match controller.execute(intent.clone()).await {
            Ok(result) => {
                // Cache successful text responses
                if let AgentResult::Text(ref text) = result {
                    let _ = state.cache.set(&payload.message, text).await;
                }
                Some(result)
            }
            Err(e) => {
                tracing::error!(trace_id = %trace_id, error = %e, "Controller execution failed");
                Some(AgentResult::Error {
                    message: e.to_string(),
                    code: "EXECUTION_ERROR".to_string(),
                })
            }
        }
    } else {
        // No controller - return intent classification only
        tracing::debug!(trace_id = %trace_id, "No controller, returning intent only");
        Some(AgentResult::Text(format!(
            "Intent classified. Controller not available in Phase 1. Intent: {:?}",
            intent
        )))
    };

    (
        StatusCode::OK,
        Json(ChatResponse {
            trace_id,
            intent,
            result,
            cached: false,
        }),
    )
}

/// Intent classification handler (for debugging/testing).
async fn intent_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IntentRequest>,
) -> impl IntoResponse {
    let trace_id = Uuid::new_v4().to_string();
    let request = NormalizedRequest::text(&payload.message);

    match state.router.classify(&request).await {
        Ok(intent) => (
            StatusCode::OK,
            Json(IntentResponse { trace_id, intent }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(IntentResponse {
                trace_id,
                intent: UserIntent::ComplexMission {
                    goal: "Error".to_string(),
                    context_summary: e.to_string(),
                    visual_refs: Vec::new(),
                },
            }),
        ),
    }
}

/// Webhook request payload.
#[derive(Debug, Deserialize)]
pub struct WebhookPayload {
    /// Arbitrary event data.
    #[serde(flatten)]
    pub data: serde_json::Value,
}

/// Webhook response.
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    /// Trace ID.
    pub trace_id: String,
    /// Whether the event was accepted.
    pub accepted: bool,
    /// Optional message.
    pub message: Option<String>,
    /// Classified intent (if processing is enabled).
    pub intent: Option<UserIntent>,
}

/// Webhook handler for system events.
///
/// Accepts events at `/v1/webhook/:event_type` and processes them
/// as system events through the normal intent routing pipeline.
async fn webhook_handler(
    State(state): State<Arc<AppState>>,
    Path(event_type): Path<String>,
    Json(payload): Json<WebhookPayload>,
) -> impl IntoResponse {
    let trace_id = Uuid::new_v4().to_string();

    tracing::info!(
        trace_id = %trace_id,
        event_type = %event_type,
        "Received webhook event"
    );

    // Create a normalized request from the system event
    let event_summary = format!(
        "System event: {} - {}",
        event_type,
        serde_json::to_string(&payload.data)
            .unwrap_or_else(|_| "invalid payload".to_string())
            .chars()
            .take(200)
            .collect::<String>()
    );

    let request = NormalizedRequest {
        trace_id: trace_id.clone(),
        content: event_summary.clone(),
        original_content: RequestContent::SystemEvent {
            event_type: event_type.clone(),
            payload: payload.data,
        },
        refs: Vec::new(),
        metadata: RequestMetadata::default(),
    };

    // Classify the event
    let intent = match state.router.classify(&request).await {
        Ok(intent) => Some(intent),
        Err(e) => {
            tracing::warn!(
                trace_id = %trace_id,
                error = %e,
                "Failed to classify webhook event"
            );
            None
        }
    };

    // For now, just acknowledge the event
    // In full implementation, we would execute via controller
    (
        StatusCode::OK,
        Json(WebhookResponse {
            trace_id,
            accepted: true,
            message: Some(format!("Event '{}' received", event_type)),
            intent,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;



    #[tokio::test]
    async fn test_health_handler() {
        let response = health_handler().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
