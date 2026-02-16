//! Encrypted secrets management for sensitive configuration.

use async_trait::async_trait;
use multi_agent_core::Result;

use serde::{Deserialize, Serialize};

/// A secret value that is encrypted at rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Secrets manager using AES-256-GCM encryption.
pub struct AesGcmSecretsManager {
    /// In-memory storage for encrypted values (for now, could be file/DB backed).
    storage: Arc<Mutex<HashMap<String, EncryptedSecret>>>,
    /// Encryption key.
    key: [u8; 32],
}

impl AesGcmSecretsManager {
    /// Create a new manager with a random key (for testing) or provided key.
    pub fn new(key: Option<[u8; 32]>) -> Self {
        let key = key.unwrap_or_else(|| {
            let mut k = [0u8; 32];
            OsRng.fill_bytes(&mut k);
            k
        });

        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
            key,
        }
    }
}

#[async_trait]
impl SecretsManager for AesGcmSecretsManager {
    async fn store(&self, key: &str, plaintext: &str) -> Result<()> {
        let cipher = Aes256Gcm::new(&self.key.into());
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| multi_agent_core::error::Error::SecurityViolation(e.to_string()))?;

        let secret = EncryptedSecret {
            ciphertext: BASE64.encode(ciphertext),
            nonce: BASE64.encode(nonce_bytes),
        };

        self.storage.lock().unwrap().insert(key.to_string(), secret);
        Ok(())
    }

    async fn retrieve(&self, key: &str) -> Result<Option<String>> {
        let storage = self.storage.lock().unwrap();
        if let Some(secret) = storage.get(key) {
            let cipher = Aes256Gcm::new(&self.key.into());

            let nonce_bytes = BASE64.decode(&secret.nonce).map_err(|e| {
                multi_agent_core::error::Error::SecurityViolation(format!("Invalid nonce: {}", e))
            })?;
            let ciphertext_bytes = BASE64.decode(&secret.ciphertext).map_err(|e| {
                multi_agent_core::error::Error::SecurityViolation(format!(
                    "Invalid ciphertext: {}",
                    e
                ))
            })?;

            let nonce = Nonce::from_slice(&nonce_bytes);

            let plaintext_bytes =
                cipher
                    .decrypt(nonce, ciphertext_bytes.as_ref())
                    .map_err(|e| {
                        multi_agent_core::error::Error::SecurityViolation(format!(
                            "Decryption failed: {}",
                            e
                        ))
                    })?;

            let plaintext = String::from_utf8(plaintext_bytes).map_err(|e| {
                multi_agent_core::error::Error::SecurityViolation(format!("Invalid UTF-8: {}", e))
            })?;

            Ok(Some(plaintext))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.storage.lock().unwrap().remove(key);
        Ok(())
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        Ok(self.storage.lock().unwrap().keys().cloned().collect())
    }
}

/// Secrets manager that persists to an encrypted file.
pub struct FilePersistentSecretsManager {
    inner: AesGcmSecretsManager,
    path: std::path::PathBuf,
}

impl FilePersistentSecretsManager {
    pub async fn new(path: impl Into<std::path::PathBuf>, key: Option<[u8; 32]>) -> Result<Self> {
        let path = path.into();
        let inner = AesGcmSecretsManager::new(key);
        
        // Load existing if available
        if path.exists() {
            let encoded = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| multi_agent_core::error::Error::storage(format!("Failed to read secrets file: {}", e)))?;
            
            // Decrypt the whole file content if needed, but here we store as JSON of EncryptedSecret
            // For simplicity in this iteration, we store the HashMap directly in the AesGcmSecretsManager
            // In a real system, the file itself would be encrypted with a master key.
            let storage: HashMap<String, EncryptedSecret> = serde_json::from_str(&encoded)
                .map_err(|e| multi_agent_core::error::Error::storage(format!("Failed to parse secrets: {}", e)))?;
            
            *inner.storage.lock().unwrap() = storage;
        }

        Ok(Self { inner, path })
    }

    async fn flush(&self) -> Result<()> {
        let encoded = {
            let storage = self.inner.storage.lock().unwrap();
            serde_json::to_string(&*storage)
                .map_err(|e| multi_agent_core::error::Error::storage(format!("Failed to serialize secrets: {}", e)))?
        };
        
        tokio::fs::write(&self.path, encoded)
            .await
            .map_err(|e| multi_agent_core::error::Error::storage(format!("Failed to write secrets file: {}", e)))?;
        
        Ok(())
    }
}

#[async_trait]
impl SecretsManager for FilePersistentSecretsManager {
    async fn store(&self, key: &str, plaintext: &str) -> Result<()> {
        self.inner.store(key, plaintext).await?;
        self.flush().await
    }

    async fn retrieve(&self, key: &str) -> Result<Option<String>> {
        self.inner.retrieve(key).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.inner.delete(key).await?;
        self.flush().await
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        self.inner.list_keys().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_persistence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        let key = [0u8; 32];

        {
            let manager = FilePersistentSecretsManager::new(path.clone(), Some(key)).await.unwrap();
            let _ : () = manager.store("test_key", "secret_value").await.unwrap();
        }

        // New manager, same file
        {
            let manager = FilePersistentSecretsManager::new(path, Some(key)).await.unwrap();
            let val: Option<String> = manager.retrieve("test_key").await.unwrap();
            assert_eq!(val, Some("secret_value".to_string()));
        }
    }
}

