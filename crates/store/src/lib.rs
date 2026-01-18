#![deny(unused)]
//! L3 Artifact Store for MutilAgent.
//!
//! This crate provides tiered storage (Hot/Warm/Cold) for artifacts,
//! implementing the pass-by-reference pattern to prevent context explosion.

pub mod memory;
pub mod redis;
pub mod s3;
pub mod vector;
pub mod qdrant;

use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;

use mutil_agent_core::{
    traits::{ArtifactMetadata, ArtifactStore, StorageTier},
    types::RefId,
    Result,
};

pub use memory::{InMemoryStore, InMemorySessionStore};
pub use redis::RedisSessionStore;
pub use s3::S3ArtifactStore;
pub use vector::SimpleVectorStore;
pub use qdrant::{QdrantMemoryStore, QdrantConfig};

/// Default threshold in bytes for pass-by-reference.
/// Content larger than this will be stored in L3 and referenced by ID.
pub const LARGE_CONTENT_THRESHOLD: usize = 1000;

/// Tiered artifact store supporting multiple storage backends.
pub struct TieredStore {
    /// Hot tier (in-memory).
    hot: Arc<dyn ArtifactStore>,
    /// Warm tier (Redis) - optional.
    warm: Option<Arc<dyn ArtifactStore>>,
    /// Cold tier (S3) - optional.
    cold: Option<Arc<dyn ArtifactStore>>,
    /// Threshold for hot storage (bytes).
    hot_threshold: usize,
    /// Threshold for warm storage (bytes).
    warm_threshold: usize,
}

impl TieredStore {
    /// Create a new tiered store with only hot tier.
    pub fn new(hot: Arc<dyn ArtifactStore>) -> Self {
        Self {
            hot,
            warm: None,
            cold: None,
            hot_threshold: 10 * 1024 * 1024,  // 10MB
            warm_threshold: 100 * 1024 * 1024, // 100MB
        }
    }

    /// Add warm tier storage.
    pub fn with_warm(mut self, warm: Arc<dyn ArtifactStore>) -> Self {
        self.warm = Some(warm);
        self
    }

    /// Add cold tier storage.
    pub fn with_cold(mut self, cold: Arc<dyn ArtifactStore>) -> Self {
        self.cold = Some(cold);
        self
    }

    /// Set hot storage threshold.
    pub fn with_hot_threshold(mut self, threshold: usize) -> Self {
        self.hot_threshold = threshold;
        self
    }

    /// Determine storage tier based on content size.
    fn select_tier(&self, size: usize) -> StorageTier {
        if size <= self.hot_threshold {
            StorageTier::Hot
        } else if size <= self.warm_threshold && self.warm.is_some() {
            StorageTier::Warm
        } else if self.cold.is_some() {
            StorageTier::Cold
        } else if self.warm.is_some() {
            StorageTier::Warm
        } else {
            StorageTier::Hot
        }
    }

    fn get_store(&self, tier: StorageTier) -> &Arc<dyn ArtifactStore> {
        match tier {
            StorageTier::Hot => &self.hot,
            StorageTier::Warm => self.warm.as_ref().unwrap_or(&self.hot),
            StorageTier::Cold => self
                .cold
                .as_ref()
                .or(self.warm.as_ref())
                .unwrap_or(&self.hot),
        }
    }
}

#[async_trait]
impl ArtifactStore for TieredStore {
    async fn save(&self, data: Bytes) -> Result<RefId> {
        let tier = self.select_tier(data.len());
        tracing::debug!(
            tier = ?tier,
            size = data.len(),
            "Saving artifact to tier"
        );
        self.get_store(tier).save(data).await
    }

    async fn save_with_type(&self, data: Bytes, content_type: &str) -> Result<RefId> {
        let tier = self.select_tier(data.len());
        tracing::debug!(
            tier = ?tier,
            size = data.len(),
            content_type = content_type,
            "Saving artifact with type to tier"
        );
        self.get_store(tier).save_with_type(data, content_type).await
    }

    async fn load(&self, id: &RefId) -> Result<Option<Bytes>> {
        // Try each tier in order
        if let Some(data) = self.hot.load(id).await? {
            return Ok(Some(data));
        }
        if let Some(ref warm) = self.warm {
            if let Some(data) = warm.load(id).await? {
                return Ok(Some(data));
            }
        }
        if let Some(ref cold) = self.cold {
            return cold.load(id).await;
        }
        Ok(None)
    }

    async fn delete(&self, id: &RefId) -> Result<()> {
        // Try to delete from all tiers
        let _ = self.hot.delete(id).await;
        if let Some(ref warm) = self.warm {
            let _ = warm.delete(id).await;
        }
        if let Some(ref cold) = self.cold {
            let _ = cold.delete(id).await;
        }
        Ok(())
    }

    async fn exists(&self, id: &RefId) -> Result<bool> {
        if self.hot.exists(id).await? {
            return Ok(true);
        }
        if let Some(ref warm) = self.warm {
            if warm.exists(id).await? {
                return Ok(true);
            }
        }
        if let Some(ref cold) = self.cold {
            return cold.exists(id).await;
        }
        Ok(false)
    }

    async fn metadata(&self, id: &RefId) -> Result<Option<ArtifactMetadata>> {
        if let Some(meta) = self.hot.metadata(id).await? {
            return Ok(Some(meta));
        }
        if let Some(ref warm) = self.warm {
            if let Some(meta) = warm.metadata(id).await? {
                return Ok(Some(meta));
            }
        }
        if let Some(ref cold) = self.cold {
            return cold.metadata(id).await;
        }
        Ok(None)
    }
}

/// Helper function to check if content should be stored by reference.
pub fn should_store_by_ref(content: &str) -> bool {
    content.len() > LARGE_CONTENT_THRESHOLD
}

/// Store large content and return a reference, or return content directly.
pub async fn maybe_store_by_ref(
    store: &dyn ArtifactStore,
    content: String,
) -> Result<(String, Option<RefId>)> {
    if should_store_by_ref(&content) {
        let ref_id = store.save(Bytes::from(content)).await?;
        let message = format!(
            "Output too large. Saved as RefID: {}. Use 'read_artifact' to view.",
            ref_id
        );
        Ok((message, Some(ref_id)))
    } else {
        Ok((content, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tiered_store() {
        let hot = Arc::new(InMemoryStore::new());
        let store = TieredStore::new(hot);

        // Save some data
        let data = Bytes::from("Hello, World!");
        let ref_id = store.save(data.clone()).await.unwrap();

        // Load it back
        let loaded = store.load(&ref_id).await.unwrap();
        assert_eq!(loaded, Some(data));

        // Check exists
        assert!(store.exists(&ref_id).await.unwrap());

        // Delete
        store.delete(&ref_id).await.unwrap();
        assert!(!store.exists(&ref_id).await.unwrap());
    }
}
