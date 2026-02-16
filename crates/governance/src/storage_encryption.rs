use async_trait::async_trait;
use multi_agent_core::traits::{ArtifactMetadata, ArtifactStore};
use multi_agent_core::types::RefId;
use multi_agent_core::Result;
use bytes::Bytes;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::{RngCore, SeedableRng};
use std::sync::Arc;

/// Wrapper that encrypts data before storing and decrypts after loading.
pub struct EncryptedArtifactStore {
    inner: Arc<dyn ArtifactStore>,
    cipher: Aes256Gcm,
}

impl EncryptedArtifactStore {
    pub fn new(inner: Arc<dyn ArtifactStore>, master_key_hex: &str) -> std::result::Result<Self, String> {
        let key_bytes = hex::decode(master_key_hex).map_err(|e| format!("Invalid hex key: {}", e))?;
        if key_bytes.len() != 32 {
            return Err("Master key must be 32 bytes (64 hex chars)".to_string());
        }
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        Ok(Self { inner, cipher })
    }

    fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut rng = rand::rngs::StdRng::from_entropy();
        let mut nonce_bytes = [0u8; 12];
        rng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self.cipher.encrypt(nonce, data)
            .map_err(|e| multi_agent_core::error::Error::Governance(format!("Encryption failed: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(multi_agent_core::error::Error::Governance("Data too short for decryption".to_string()));
        }

        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| multi_agent_core::error::Error::Governance(format!("Decryption failed: {}", e)))?;

        Ok(plaintext)
    }
}

#[async_trait]
impl ArtifactStore for EncryptedArtifactStore {
    async fn save(&self, data: Bytes) -> Result<RefId> {
        let encrypted = self.encrypt(&data)?;
        self.inner.save(Bytes::from(encrypted)).await
    }

    async fn save_with_id(&self, id: &RefId, data: Bytes) -> Result<()> {
        let encrypted = self.encrypt(&data)?;
        self.inner.save_with_id(id, Bytes::from(encrypted)).await
    }

    async fn save_with_type(&self, data: Bytes, content_type: &str) -> Result<RefId> {
        let encrypted = self.encrypt(&data)?;
        self.inner.save_with_type(Bytes::from(encrypted), content_type).await
    }

    async fn load(&self, id: &RefId) -> Result<Option<Bytes>> {
        match self.inner.load(id).await? {
            Some(encrypted) => {
                let decrypted = self.decrypt(&encrypted)?;
                Ok(Some(Bytes::from(decrypted)))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &RefId) -> Result<()> {
        self.inner.delete(id).await
    }

    async fn exists(&self, id: &RefId) -> Result<bool> {
        self.inner.exists(id).await
    }

    async fn metadata(&self, id: &RefId) -> Result<Option<ArtifactMetadata>> {
        // Metadata is not encrypted, but size will reflect encrypted size
        self.inner.metadata(id).await
    }

    async fn health_check(&self) -> Result<()> {
        self.inner.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multi_agent_store::InMemoryStore;

    #[tokio::test]
    async fn test_encryption_roundtrip() {
        let base_store = Arc::new(InMemoryStore::new());
        
        // 32-byte key in hex
        let key = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let store = EncryptedArtifactStore::new(base_store.clone(), key).unwrap();

        let data = Bytes::from("SECRET DATA");
        let id = store.save(data.clone()).await.unwrap();

        // Load via encrypted store -> should be plaintext
        let loaded = store.load(&id).await.unwrap().unwrap();
        assert_eq!(loaded, data);

        // Load via base store -> should be ciphertext (different from plaintext)
        let raw: Bytes = base_store.load(&id).await.unwrap().unwrap();
        assert_ne!(raw, data);
        assert!(raw.len() > data.len()); // Nonce + Auth Tag overhead
    }
}
