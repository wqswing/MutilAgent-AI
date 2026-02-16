//! Distributed tracing configuration.

use multi_agent_core::{Error, Result};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Configure distributed tracing with OpenTelemetry and stdout logging.
pub fn configure_tracing() -> Result<()> {
    // Basic EnvFilter
    let env_filter = tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info,multiagent=debug".into()),
    );

    // Stdout formatting layer
    let fmt_layer = tracing_subscriber::fmt::layer();

    // Registry with fmt and filter
    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    // Check OTLP endpoint
    if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        tracing::info!(endpoint = %endpoint, "Initializing OpenTelemetry tracing");

        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint)
            .build_span_exporter()
            .map_err(|e| Error::governance(format!("Failed to create OTLP exporter: {}", e)))?;

        let resource = Resource::new(vec![KeyValue::new(
            "service.name",
            "multiagent-gateway",
        )]);

        let provider = sdktrace::TracerProvider::builder()
            .with_batch_exporter(exporter, runtime::Tokio)
            .with_config(sdktrace::Config::default().with_resource(resource))
            .build();

        use opentelemetry::trace::TracerProvider;
        let tracer = provider.tracer("multiagent-gateway");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        registry.with(otel_layer).init();
    } else {
        registry.init();
    }

    Ok(())
}
