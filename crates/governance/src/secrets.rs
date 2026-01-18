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
            
            let nonce_bytes = BASE64
                .decode(&secret.nonce)
                .map_err(|e| multi_agent_core::error::Error::SecurityViolation(format!("Invalid nonce: {}", e)))?;
            let ciphertext_bytes = BASE64
                .decode(&secret.ciphertext)
                .map_err(|e| multi_agent_core::error::Error::SecurityViolation(format!("Invalid ciphertext: {}", e)))?;
                
            let nonce = Nonce::from_slice(&nonce_bytes);
            
            let plaintext_bytes = cipher
                .decrypt(nonce, ciphertext_bytes.as_ref())
                .map_err(|e| multi_agent_core::error::Error::SecurityViolation(format!("Decryption failed: {}", e)))?;
                
            let plaintext = String::from_utf8(plaintext_bytes)
                .map_err(|e| multi_agent_core::error::Error::SecurityViolation(format!("Invalid UTF-8: {}", e)))?;
                
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
