use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use multi_agent_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub providers: Vec<ProviderDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefinition {
    pub name: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub models: Vec<ModelDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub id: String,
    pub cost_in: Option<f64>,
    pub cost_out: Option<f64>,
    pub capabilities: Vec<String>,
    pub max_tokens: Option<u32>,
}

impl ProviderConfig {
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path).await
            .map_err(|e| multi_agent_core::Error::gateway(format!("Failed to read provider config: {}", e)))?;
        
        let config: Self = serde_json::from_str(&content)
            .map_err(|e| multi_agent_core::Error::gateway(format!("Failed to parse provider config: {}", e)))?;
            
        Ok(config)
    }
}
