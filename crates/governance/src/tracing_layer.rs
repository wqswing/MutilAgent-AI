//! Distributed tracing configuration.

use multi_agent_core::{Error, Result};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Configure distributed tracing with OpenTelemetry and stdout logging.
pub fn configure_tracing(
    rust_log: Option<&str>,
    otel_endpoint: Option<&str>,
    json_logs: bool,
) -> Result<()> {
    // Basic EnvFilter
    let env_filter =
        tracing_subscriber::EnvFilter::new(rust_log.unwrap_or("info,multiagent=debug"));

    // Registry with env filter
    let registry = tracing_subscriber::registry().with(env_filter);

    // OTLP Tracer Setup
    let tracer = if let Some(endpoint) = otel_endpoint {
        tracing::info!(endpoint = %endpoint, "Initializing OpenTelemetry tracing");

        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint)
            .build_span_exporter()
            .map_err(|e| Error::governance(format!("Failed to create OTLP exporter: {}", e)))?;

        let resource = Resource::new(vec![KeyValue::new("service.name", "multiagent-gateway")]);

        let provider = sdktrace::TracerProvider::builder()
            .with_batch_exporter(exporter, runtime::Tokio)
            .with_config(sdktrace::Config::default().with_resource(resource))
            .build();

        use opentelemetry::trace::TracerProvider;
        Some(provider.tracer("multiagent-gateway"))
    } else {
        None
    };

    if json_logs {
        let fmt_layer = tracing_subscriber::fmt::layer().json();
        if let Some(tracer) = tracer {
            let otel = tracing_opentelemetry::layer().with_tracer(tracer);
            registry.with(fmt_layer).with(otel).init();
        } else {
            registry.with(fmt_layer).init();
        }
    } else {
        let fmt_layer = tracing_subscriber::fmt::layer();
        if let Some(tracer) = tracer {
            let otel = tracing_opentelemetry::layer().with_tracer(tracer);
            registry.with(fmt_layer).with(otel).init();
        } else {
            registry.with(fmt_layer).init();
        }
    }

    Ok(())
}
