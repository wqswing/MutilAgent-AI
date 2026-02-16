#![deny(unused)]
//! Multiagent - Multi-Agent AI System
//!
//! A layered, Rust-based multi-agent architecture supporting multi-modal ingestion,
//! intelligent routing, ReAct-based orchestration, and production-grade resilience.

use multi_agent_sandbox::SandboxEngine;
use std::sync::Arc;

use multi_agent_controller::ReActController;
use multi_agent_core::traits::{ArtifactStore, SessionStore, ToolRegistry};
use multi_agent_gateway::{DefaultRouter, GatewayConfig, GatewayServer, InMemorySemanticCache};
use multi_agent_skills::{CalculatorTool, DefaultToolRegistry, EchoTool};
use multi_agent_store::{
    InMemorySessionStore, InMemoryStore, RedisSessionStore, S3ArtifactStore, TieredStore,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    // Initialize tracing
    let rust_log = std::env::var("RUST_LOG").ok(); // Still allow env override for RUST_LOG as it's common
    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    multi_agent_governance::configure_tracing(rust_log.as_deref(), otel_endpoint.as_deref())?;

    tracing::info!("Starting Multiagent v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let app_config = multi_agent_core::config::AppConfig::load()?;

    // =========================================================================
    // Initialize L3: Artifact Store
    // =========================================================================
    let store: Arc<dyn ArtifactStore> = if let Some(bucket) = &app_config.store.s3_bucket {
        let endpoint = app_config.store.s3_endpoint.as_deref();
        tracing::info!(bucket = %bucket, endpoint = ?endpoint, "Initializing S3 Artifact Store (Tiered)");

        let s3 = Arc::new(S3ArtifactStore::new(bucket, "", endpoint).await);
        let hot = Arc::new(InMemoryStore::new());
        Arc::new(TieredStore::new(hot).with_cold(s3))
    } else {
        tracing::info!("Initializing In-Memory Artifact Store");
        Arc::new(InMemoryStore::new())
    };

    // Data-at-rest Encryption
    let store = if app_config.store.encryption.enabled {
        if let Some(key) = &app_config.store.encryption.master_key {
            tracing::info!("🔒 Artifact Store Encryption ENABLED");
            Arc::new(multi_agent_governance::EncryptedArtifactStore::new(store, key)
                .map_err(|e| multi_agent_core::Error::governance(format!("Encryption init failed: {}", e)))?)
        } else {
            tracing::warn!("Encryption enabled but no master key provided - falling back to plaintext");
            store
        }
    } else {
        store
    };

    // Initialize Session Store
    let session_store: Arc<dyn SessionStore> = if let Some(redis_url) = &app_config.store.redis_url
    {
        tracing::info!(url = %redis_url, "Initializing Redis Session Store");
        Arc::new(RedisSessionStore::new(
            redis_url,
            "multiagent:session",
            3600 * 24,
        )?)
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

    // =========================================================================
    // Initialize Sandbox (Sovereign Execution Plane)
    // =========================================================================
    let sandbox_manager = match multi_agent_sandbox::DockerSandbox::new() {
        Ok(engine) => {
            let engine = std::sync::Arc::new(engine);
            if engine.is_available().await {
                let config = multi_agent_sandbox::SandboxConfig::default();
                let manager = Arc::new(multi_agent_sandbox::SandboxManager::new(engine, config));

                // Register sandbox tools
                tools
                    .register(Box::new(multi_agent_sandbox::SandboxShellTool::new(
                        manager.clone(),
                    )))
                    .await?;
                tools
                    .register(Box::new(multi_agent_sandbox::SandboxWriteFileTool::new(
                        manager.clone(),
                    )))
                    .await?;
                tools
                    .register(Box::new(multi_agent_sandbox::SandboxReadFileTool::new(
                        manager.clone(),
                    )))
                    .await?;
                tools
                    .register(Box::new(multi_agent_sandbox::SandboxListFilesTool::new(
                        manager.clone(),
                    )))
                    .await?;

                tracing::info!("🐳 Sovereign Sandbox initialized (Docker available)");
                Some(manager)
            } else {
                tracing::warn!("Docker daemon not reachable — sandbox tools disabled");
                None
            }
        }
        Err(e) => {
            tracing::warn!("Docker not available ({}). Sandbox tools disabled.", e);
            None
        }
    };

    // Network Policy setup
    let network_policy = Arc::new(multi_agent_governance::network::NetworkPolicy::new(
        app_config.governance.allow_domains.clone(),
        app_config.governance.deny_domains.clone(),
        vec![80, 443],
    ));

    // Register Network tools
    tools.register(Box::new(multi_agent_skills::network::FetchTool::new(network_policy.clone())) as Box<dyn multi_agent_core::traits::Tool>).await?;
    if let Some(sm) = &sandbox_manager {
        tools.register(Box::new(multi_agent_skills::network::DownloadTool::new(network_policy.clone(), sm.clone())) as Box<dyn multi_agent_core::traits::Tool>).await?;
    }

    let _sandbox_manager = sandbox_manager; // keep alive

    tracing::info!(tools_count = tools.len(), "L2 Skills registry initialized");

    // =========================================================================
    // Initialize L1: Controller
    // =========================================================================
    let controller = Arc::new(
        ReActController::builder()
            .with_store(store.clone())
            .with_session_store(session_store.clone())
            .build(),
    );
    tracing::info!("L1 Controller initialized (mock ReAct)");

    // =========================================================================
    // Initialize L0: Gateway
    // =========================================================================
    let router = Arc::new(DefaultRouter::new());

    // Initialize LLM Client for embeddings
    use multi_agent_core::traits::LlmClient;

    let llm_client: Arc<dyn LlmClient> = {
        let providers_path = std::path::Path::new("providers.json");
        if providers_path.exists() {
            tracing::info!("Loading LLM config from providers.json");
            match multi_agent_model_gateway::config::ProviderConfig::load(providers_path).await {
                Ok(cfg) => {
                    match multi_agent_model_gateway::create_client_from_config(
                        &cfg,
                        app_config.model_gateway.openai_api_key.clone(),
                        app_config.model_gateway.anthropic_api_key.clone(),
                    ) {
                        Ok(client) => Arc::new(client),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to create client from config: {}. Fallback to env vars.",
                                e
                            );
                            match multi_agent_model_gateway::create_default_client() {
                                Ok(client) => Arc::new(client),
                                Err(_) => {
                                    Arc::new(multi_agent_model_gateway::MockLlmClient::new("dummy"))
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to parse providers.json: {}. Fallback to env vars.",
                        e
                    );
                    match multi_agent_model_gateway::create_default_client() {
                        Ok(client) => Arc::new(client),
                        Err(_) => Arc::new(multi_agent_model_gateway::MockLlmClient::new("dummy")),
                    }
                }
            }
        } else {
            tracing::info!("No providers.json found. Using environment variables.");
            match multi_agent_model_gateway::create_default_client() {
                Ok(client) => Arc::new(client),
                Err(e) => {
                    tracing::warn!("Failed to create default LLM client: {}. Semantic cache will fallback to exact match.", e);
                    Arc::new(multi_agent_model_gateway::MockLlmClient::new("dummy"))
                }
            }
        }
    };

    let cache = Arc::new(InMemorySemanticCache::new(llm_client));

    let gateway_config = GatewayConfig {
        host: app_config.server.host.clone(),
        port: app_config.server.port,
        enable_cors: true,
        enable_tracing: true,
        allowed_origins: app_config.gateway.allowed_origins.clone(),
        tls: app_config.gateway.tls.clone(),
    };

    let server =
        GatewayServer::new(gateway_config.clone(), router, cache).with_controller(controller);

    tracing::info!(
        host = %gateway_config.host,
        port = gateway_config.port,
        "L0 Gateway initialized"
    );

    // =========================================================================
    // Print startup banner
    // =========================================================================
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!(
        "║                     Multiagent v{}                       ║",
        env!("CARGO_PKG_VERSION")
    );
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Multi-Agent AI System - Phase 1 (Core Foundation)           ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Endpoints:                                                   ║");
    println!("║    GET  /health      - Health check                          ║");
    println!("║    POST /v1/chat     - Chat with the agent                   ║");
    println!("║    POST /v1/intent   - Classify intent only                  ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║  Server: http://{}:{}                              ║",
        gateway_config.host, gateway_config.port
    );
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // =========================================================================
    // Initialize L4: Observability (Metrics) & Governance
    // =========================================================================
    let metrics_handle = multi_agent_governance::setup_metrics_recorder()?;

    // Initialize Governance Components
    let audit_store = Arc::new(multi_agent_governance::SqliteAuditStore::new(
        &app_config.governance.audit_log_path,
    )?);

    // Secrets manager for encrypting API keys
    // In prod, key should come from Kms/Env. For now, random.
    let secrets_path = std::path::PathBuf::from("secrets.json");
    let secrets_manager: Arc<dyn multi_agent_governance::SecretsManager> =
        Arc::new(multi_agent_governance::secrets::FilePersistentSecretsManager::new(secrets_path, None).await?);

    // RBAC: Check environment for production mode
    let is_production = app_config.governance.multiagent_env.to_lowercase() == "production";

    let rbac: Arc<dyn multi_agent_governance::RbacConnector> = if is_production {
        // Production: Require OIDC configuration
        let oidc_issuer = app_config.governance.oidc_issuer.as_ref()
            .expect("OIDC_ISSUER is required in production mode. Set governance.multiagent_env=development to disable.");
        tracing::info!(issuer = %oidc_issuer, "Initializing OIDC RBAC connector for production");
        Arc::new(multi_agent_governance::rbac::OidcRbacConnector::new(
            oidc_issuer,
        ))
    } else {
        tracing::warn!("Using NoOpRbacConnector - NOT SUITABLE FOR PRODUCTION");
        Arc::new(multi_agent_governance::NoOpRbacConnector)
    };

    // Initialize MCP Registry
    let mcp_registry = Arc::new(multi_agent_skills::McpRegistry::new());
    mcp_registry.register_defaults(); // Register built-in defaults

    // Initialize Redis components if configured
    let redis_url = app_config.store.redis_url.as_ref();

    let (provider_store, rate_limiter) = if let Some(url) = redis_url {
        tracing::info!("Initializing Redis backends at {}", url);

        let provider_store = match multi_agent_store::RedisProviderStore::new(url, "providers") {
            Ok(store) => Some(Arc::new(store) as Arc<dyn multi_agent_core::traits::ProviderStore>),
            Err(e) => {
                tracing::error!("Failed to initialize RedisProviderStore: {}", e);
                None
            }
        };

        let rate_limiter = match multi_agent_store::RedisRateLimiter::new(url) {
            Ok(limiter) => {
                Some(Arc::new(limiter) as Arc<dyn multi_agent_core::traits::DistributedRateLimiter>)
            }
            Err(e) => {
                tracing::error!("Failed to initialize RedisRateLimiter: {}", e);
                None
            }
        };

        (provider_store, rate_limiter)
    } else {
        tracing::info!("REDIS_URL not set - using in-memory stores");
        (None, None)
    };

    let admin_state = Arc::new(multi_agent_admin::AdminState {
        audit_store,
        rbac,
        metrics: Some(metrics_handle.clone()),
        mcp_registry: mcp_registry.clone(),
        providers: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        provider_store,
        secrets: secrets_manager,
        artifact_store: Some(store.clone()),
        session_store: Some(session_store.clone()),
        privacy_controller: None,
        app_config: app_config.clone(),
    });

    // =========================================================================
    // Start the server
    // =========================================================================
    let mut server = server.with_metrics(metrics_handle).with_admin(admin_state);

    if let Some(limiter) = rate_limiter {
        server = server.with_rate_limiter(limiter);
    }

    server.run().await?;

    Ok(())
}
