use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub gateway: GatewayConfig,
    pub controller: ControllerConfig,
    pub store: StoreConfig,
    pub governance: GovernanceConfig,
    pub model_gateway: ModelGatewayConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GatewayConfig {
    pub routing_timeout_ms: u64,
    pub semantic_cache_threshold: f64,
    pub allowed_origins: Vec<String>,
    pub tls: TlsConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub ca_path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ControllerConfig {
    pub max_react_iterations: u32,
    pub state_persistence: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StoreConfig {
    pub large_content_threshold: usize,
    pub default_tier: String,
    pub s3_bucket: Option<String>,
    pub s3_endpoint: Option<String>,
    pub redis_url: Option<String>,
    pub encryption: EncryptionConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EncryptionConfig {
    pub enabled: bool,
    pub master_key: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GovernanceConfig {
    pub default_token_budget: u32,
    pub enable_tracing: bool,
    pub audit_log_path: String,
    pub audit_log_storage_path: String,
    pub multiagent_env: String,
    pub oidc_issuer: Option<String>,
    pub admin_token: Option<String>,
    pub allow_domains: Vec<String>,
    pub deny_domains: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelGatewayConfig {
    pub default_provider: String,
    pub fallback_enabled: bool,
    pub providers: std::collections::HashMap<String, ProviderConfig>,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub models: Vec<String>,
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let env = std::env::var("MULTIAGENT_ENV").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            .add_source(File::with_name("config/default"))
            .add_source(File::with_name(&format!("config/{}", env)).required(false))
            .add_source(File::with_name("config/local").required(false))
            // Map APP__SERVER__PORT=3000 to app.server.port
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 3000,
            },
            gateway: GatewayConfig {
                routing_timeout_ms: 5000,
                semantic_cache_threshold: 0.85,
                allowed_origins: vec!["*".into()],
                tls: TlsConfig {
                    enabled: false,
                    cert_path: None,
                    key_path: None,
                    ca_path: None,
                },
            },
            controller: ControllerConfig {
                max_react_iterations: 10,
                state_persistence: false,
            },
            store: StoreConfig {
                large_content_threshold: 1048576,
                default_tier: "local".into(),
                s3_bucket: None,
                s3_endpoint: None,
                redis_url: None,
                encryption: EncryptionConfig {
                    enabled: false,
                    master_key: None,
                },
            },
            governance: GovernanceConfig {
                default_token_budget: 100000,
                enable_tracing: false,
                audit_log_path: "/tmp/audit.log".into(),
                audit_log_storage_path: "/tmp/audit_storage".into(),
                multiagent_env: "test".into(),
                oidc_issuer: None,
                admin_token: None,
                allow_domains: vec!["*.openai.com".into(), "*.anthropic.com".into()],
                deny_domains: vec![],
            },
            model_gateway: ModelGatewayConfig {
                default_provider: "openai".into(),
                fallback_enabled: false,
                providers: std::collections::HashMap::new(),
                openai_api_key: None,
                anthropic_api_key: None,
            },
        }
    }
}
