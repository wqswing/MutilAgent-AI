//! L3 Store traits.

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::error::Result;
use crate::types::RefId;

/// Artifact store for managing large content.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// Save data and return a reference ID.
    async fn save(&self, data: Bytes) -> Result<RefId>;

    /// Save data with a specific content type.
    async fn save_with_type(&self, data: Bytes, content_type: &str) -> Result<RefId>;

    /// Load data by reference ID.
    async fn load(&self, id: &RefId) -> Result<Option<Bytes>>;

    /// Delete an artifact.
    async fn delete(&self, id: &RefId) -> Result<()>;

    /// Check if an artifact exists.
    async fn exists(&self, id: &RefId) -> Result<bool>;

    /// Get metadata about an artifact.
    async fn metadata(&self, id: &RefId) -> Result<Option<ArtifactMetadata>>;
}

/// Metadata for stored artifacts.
#[derive(Debug, Clone)]
pub struct ArtifactMetadata {
    /// Size in bytes.
    pub size: usize,
    /// Content type.
    pub content_type: String,
    /// Creation timestamp.
    pub created_at: i64,
    /// Storage tier.
    pub tier: StorageTier,
}

/// Storage tier for tiered storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTier {
    /// Hot storage (in-memory, fastest).
    Hot,
    /// Warm storage (Redis, fast).
    Warm,
    /// Cold storage (S3, cheapest).
    Cold,
}

/// A single entry in the long-term memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique ID of the entry.
    pub id: String,
    /// The text content of the memory.
    pub content: String,
    /// The vector embedding of the content.
    pub embedding: Vec<f32>,
    /// Metadata (e.g., origin, timestamp, tags).
    pub metadata: HashMap<String, String>,
}

/// Interface for vector database operations.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Add a new entry to memory.
    async fn add(&self, entry: MemoryEntry) -> Result<()>;

    /// Search for similar entries.
    async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<MemoryEntry>>;

    /// Delete an entry by ID.
    async fn delete(&self, id: &str) -> Result<()>;
}
