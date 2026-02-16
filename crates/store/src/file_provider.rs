use async_trait::async_trait;
use multi_agent_core::{traits::ProviderEntry, traits::ProviderStore, Result};
use std::path::PathBuf;

/// Persistent storage for LLM provider configurations using a JSON file.
pub struct FileProviderStore {
    path: PathBuf,
}

impl FileProviderStore {
    /// Create a new file-based provider store.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl ProviderStore for FileProviderStore {
    async fn list(&self) -> Result<Vec<ProviderEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&self.path).map_err(|e| {
            multi_agent_core::Error::storage(format!("Failed to read provider file: {}", e))
        })?;
        let providers: Vec<ProviderEntry> = serde_json::from_str(&content).map_err(|e| {
            multi_agent_core::Error::storage(format!("Failed to parse provider file: {}", e))
        })?;
        Ok(providers)
    }

    async fn get(&self, id: &str) -> Result<Option<ProviderEntry>> {
        let providers = self.list().await?;
        Ok(providers.into_iter().find(|p| p.id == id))
    }

    async fn upsert(&self, provider: &ProviderEntry) -> Result<()> {
        let mut providers = self.list().await?;
        if let Some(existing) = providers.iter_mut().find(|p| p.id == provider.id) {
            *existing = provider.clone();
        } else {
            providers.push(provider.clone());
        }
        let content = serde_json::to_string_pretty(&providers).map_err(|e| {
            multi_agent_core::Error::storage(format!("Failed to serialize providers: {}", e))
        })?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                multi_agent_core::Error::storage(format!(
                    "Failed to create provider directory: {}",
                    e
                ))
            })?;
        }
        std::fs::write(&self.path, content).map_err(|e| {
            multi_agent_core::Error::storage(format!("Failed to write provider file: {}", e))
        })?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let mut providers = self.list().await?;
        let len_before = providers.len();
        providers.retain(|p| p.id != id);
        if providers.len() == len_before {
            return Ok(false);
        }
        let content = serde_json::to_string_pretty(&providers).map_err(|e| {
            multi_agent_core::Error::storage(format!("Failed to serialize providers: {}", e))
        })?;
        std::fs::write(&self.path, content).map_err(|e| {
            multi_agent_core::Error::storage(format!("Failed to write provider file: {}", e))
        })?;
        Ok(true)
    }
}
