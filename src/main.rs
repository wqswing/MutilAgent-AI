#![deny(unused)]
//! Multiagent - Multi-Agent AI System
//!
//! A layered, Rust-based multi-agent architecture supporting multi-modal ingestion,
//! intelligent routing, ReAct-based orchestration, and production-grade resilience.

use std::sync::Arc;

use multi_agent_core::traits::{ToolRegistry, ArtifactStore, SessionStore};
use multi_agent_controller::ReActController;
use multi_agent_gateway::{DefaultRouter, GatewayConfig, GatewayServer, InMemorySemanticCache};
use multi_agent_skills::{DefaultToolRegistry, EchoTool, CalculatorTool};
use multi_agent_store::{InMemoryStore, InMemorySessionStore, RedisSessionStore, S3ArtifactStore, TieredStore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    // Initialize tracing
    multi_agent_governance::configure_tracing()?;


    tracing::info!("Starting Multiagent v{}", env!("CARGO_PKG_VERSION"));

    // =========================================================================
    // Initialize L3: Artifact Store
    // =========================================================================
    // =========================================================================
    // Initialize L3: Artifact Store
    // =========================================================================
    let store: Arc<dyn ArtifactStore> = if let Ok(bucket) = std::env::var("AWS_S3_BUCKET") {
        tracing::info!(bucket = %bucket, "Initializing S3 Artifact Store (Tiered)");
        let s3 = Arc::new(S3ArtifactStore::new(&bucket, "").await);
        let hot = Arc::new(InMemoryStore::new());
        Arc::new(TieredStore::new(hot).with_cold(s3))
    } else {
        tracing::info!("Initializing In-Memory Artifact Store");
        Arc::new(InMemoryStore::new())
    };

    // Initialize Session Store
    let session_store: Arc<dyn SessionStore> = if let Ok(redis_url) = std::env::var("REDIS_URL") {
        tracing::info!(url = %redis_url, "Initializing Redis Session Store");
        Arc::new(RedisSessionStore::new(&redis_url, "multiagent:session", 3600 * 24)?)
    } else {
        tracing::info!("Initializing In-Memory Session Store");
        Arc::new(InMemorySessionStore::new())
    };

    // =========================================================================
    // Initialize L2: Skills & Tools
    // =========================================================================
    let tools = Arc::new(DefaultToolRegistry::new());
    
    // Register built-in tools
    tools.register(Box::new(EchoTool)).await?;
    tools.register(Box::new(CalculatorTool)).await?;
    
    tracing::info!(
        tools_count = tools.len(),
        "L2 Skills registry initialized"
    );

    // =========================================================================
    // Initialize L1: Controller
    // =========================================================================
    let controller = Arc::new(
        ReActController::builder()
            .with_store(store.clone())
            .with_session_store(session_store.clone())
            .build()
    );
    tracing::info!("L1 Controller initialized (mock ReAct)");

    // =========================================================================
    // Initialize L0: Gateway
    // =========================================================================
    let router = Arc::new(DefaultRouter::new());
    
    // Initialize LLM Client for embeddings
    use multi_agent_core::traits::LlmClient;
    let llm_client: Arc<dyn LlmClient> = match multi_agent_model_gateway::create_default_client() {
        Ok(client) => Arc::new(client),
        Err(e) => {
            tracing::warn!("Failed to create default LLM client: {}. Semantic cache will fallback to exact match.", e);
            Arc::new(multi_agent_model_gateway::MockLlmClient::new("dummy"))
        }
    };
    
    let cache = Arc::new(InMemorySemanticCache::new(llm_client));
    
    let config = GatewayConfig {
        host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
        port: std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000),
        enable_cors: true,
        enable_tracing: true,
    };

    let server = GatewayServer::new(config.clone(), router, cache)
        .with_controller(controller);

    tracing::info!(
        host = %config.host,
        port = config.port,
        "L0 Gateway initialized"
    );

    // =========================================================================
    // Print startup banner
    // =========================================================================
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     Multiagent v{}                       ║", env!("CARGO_PKG_VERSION"));
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Multi-Agent AI System - Phase 1 (Core Foundation)           ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Endpoints:                                                   ║");
    println!("║    GET  /health      - Health check                          ║");
    println!("║    POST /v1/chat     - Chat with the agent                   ║");
    println!("║    POST /v1/intent   - Classify intent only                  ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Server: http://{}:{}                              ║", config.host, config.port);
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // =========================================================================
    // Initialize L4: Observability (Metrics)
    // =========================================================================
    let metrics_handle = multi_agent_governance::setup_metrics_recorder()?;
    
    // =========================================================================
    // Start the server
    // =========================================================================
    server
        .with_metrics(metrics_handle)
        .run()
        .await?;

    Ok(())
}
