use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Version identifier for Gateway typed contract.
pub const GATEWAY_CONTRACT_VERSION: &str = "v1";

/// Standard typed response envelope for Gateway APIs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiEnvelope<T> {
    pub version: String,
    pub trace_id: String,
    pub data: T,
}

impl<T> ApiEnvelope<T> {
    pub fn success(trace_id: impl Into<String>, data: T) -> Self {
        Self {
            version: GATEWAY_CONTRACT_VERSION.to_string(),
            trace_id: trace_id.into(),
            data,
        }
    }
}

/// Stable API error code catalog for clients.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiErrorCode {
    InvalidRequest,
    RoutingFailed,
    ControllerFailed,
    Unauthorized,
    Forbidden,
    Conflict,
    InternalError,
}

/// Standardized typed API error body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiErrorBody {
    pub code: ApiErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiErrorBody {
    pub fn new(code: ApiErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_contract_envelope_serialization_is_stable() {
        let envelope = ApiEnvelope::success(
            "trace-123",
            serde_json::json!({
                "intent": "complex_mission",
            }),
        );
        let value = serde_json::to_value(envelope).expect("serialize envelope");

        assert_eq!(value["version"], "v1");
        assert_eq!(value["trace_id"], "trace-123");
        assert_eq!(value["data"]["intent"], "complex_mission");
    }

    #[test]
    fn gateway_contract_error_serialization_is_stable() {
        let err = ApiErrorBody::new(ApiErrorCode::RoutingFailed, "router failed", true)
            .with_details(serde_json::json!({ "source": "llm" }));
        let value = serde_json::to_value(err).expect("serialize error");

        assert_eq!(value["code"], "ROUTING_FAILED");
        assert_eq!(value["message"], "router failed");
        assert_eq!(value["retryable"], true);
        assert_eq!(value["details"]["source"], "llm");
    }
}
