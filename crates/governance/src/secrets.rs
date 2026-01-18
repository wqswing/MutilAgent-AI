//! Encrypted secrets management for sensitive configuration.

use async_trait::async_trait;
use multi_agent_core::Result;

/// A secret value that is encrypted at rest.
#[derive(Debug, Clone)]
pub struct EncryptedSecret {
    /// The encrypted data (base64 encoded).
    pub ciphertext: String,
    /// Nonce used for encryption (base64 encoded).
    pub nonce: String,
}

/// Trait for secrets management with encryption.
#[async_trait]
pub trait SecretsManager: Send + Sync {
    /// Store a secret with encryption.
    async fn store(&self, key: &str, plaintext: &str) -> Result<()>;
    
    /// Retrieve and decrypt a secret.
    async fn retrieve(&self, key: &str) -> Result<Option<String>>;
    
    /// Delete a secret.
    async fn delete(&self, key: &str) -> Result<()>;
    
    /// List all secret keys (not values).
    async fn list_keys(&self) -> Result<Vec<String>>;
}

/// In-memory secrets manager for testing (stores plaintext, NOT for production).
pub struct InMemorySecretsManager {
    secrets: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl InMemorySecretsManager {
    pub fn new() -> Self {
        Self {
            secrets: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemorySecretsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretsManager for InMemorySecretsManager {
    async fn store(&self, key: &str, plaintext: &str) -> Result<()> {
        self.secrets.lock().unwrap().insert(key.to_string(), plaintext.to_string());
        Ok(())
    }
    
    async fn retrieve(&self, key: &str) -> Result<Option<String>> {
        Ok(self.secrets.lock().unwrap().get(key).cloned())
    }
    
    async fn delete(&self, key: &str) -> Result<()> {
        self.secrets.lock().unwrap().remove(key);
        Ok(())
    }
    
    async fn list_keys(&self) -> Result<Vec<String>> {
        Ok(self.secrets.lock().unwrap().keys().cloned().collect())
    }
}
