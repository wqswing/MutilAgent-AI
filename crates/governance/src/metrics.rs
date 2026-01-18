//! Metrics implementation using Prometheus.

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use multi_agent_core::{Error, Result};

/// Initialize Prometheus recorder and return the handle.
pub fn setup_metrics_recorder() -> Result<PrometheusHandle> {
    let builder = PrometheusBuilder::new();
    
    let handle = builder
        .install_recorder()
        .map_err(|e| Error::governance(format!("Failed to install Prometheus recorder: {}", e)))?;
        
    tracing::info!("Prometheus metrics recorder initialized");
    Ok(handle)
}

/// Helper to track HTTP request metrics (latency, count).
pub fn track_request(method: &str, path: &str, status: u16, latency_sec: f64) {
    metrics::counter!(
        "http_requests_total",
        "method" => method.to_string(),
        "path" => path.to_string(),
        "status" => status.to_string()
    )
    .increment(1);

    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method.to_string(),
        "path" => path.to_string()
    )
    .record(latency_sec);
}

/// Helper to track token usage.
pub fn track_tokens(model: &str, prompt: u64, completion: u64) {
    metrics::counter!("llm_token_usage_total", "model" => model.to_string(), "type" => "prompt").increment(prompt);
    metrics::counter!("llm_token_usage_total", "model" => model.to_string(), "type" => "completion").increment(completion);
}
