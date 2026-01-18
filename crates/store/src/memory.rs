//! In-memory artifact store implementation using DashMap.

use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use multi_agent_core::{
    traits::{ArtifactMetadata, ArtifactStore, StorageTier, SessionStore},
    types::{RefId, Session, SessionStatus}, Result,
};

/// Stored artifact with metadata.
#[derive(Debug, Clone)]
struct StoredArtifact {
    /// The actual data.
    data: Bytes,
    /// Content type.
    content_type: String,
    /// Creation timestamp.
    created_at: i64,
}

/// In-memory artifact store using DashMap for concurrent access.
///
/// This is the "Hot" tier storage, providing the fastest access
/// at the cost of memory usage.
#[derive(Debug)]
pub struct InMemoryStore {
    /// Thread-safe concurrent hashmap.
    data: DashMap<String, StoredArtifact>,
}

impl InMemoryStore {
    /// Create a new in-memory store.
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
        }
    }

    /// Get the number of stored artifacts.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all artifacts.
    pub fn clear(&self) {
        self.data.clear();
    }

    /// Get total memory usage in bytes (approximate).
    pub fn memory_usage(&self) -> usize {
        self.data.iter().map(|r| r.value().data.len()).sum()
    }

    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory session store.
pub struct InMemorySessionStore {
    sessions: DashMap<String, Session>,
}

impl InMemorySessionStore {
    /// Create a new in-memory session store.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn save(&self, session: &Session) -> Result<()> {
        self.sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn load(&self, session_id: &str) -> Result<Option<Session>> {
        Ok(self.sessions.get(session_id).map(|r| r.clone()))
    }

    async fn delete(&self, session_id: &str) -> Result<()> {
        self.sessions.remove(session_id);
        Ok(())
    }

    async fn list_running(&self) -> Result<Vec<String>> {
        Ok(self
            .sessions
            .iter()
            .filter(|r| r.status == SessionStatus::Running)
            .map(|r| r.key().clone())
            .collect())
    }
}

#[async_trait]
impl ArtifactStore for InMemoryStore {
    async fn save(&self, data: Bytes) -> Result<RefId> {
        self.save_with_type(data, "application/octet-stream").await
    }

    async fn save_with_type(&self, data: Bytes, content_type: &str) -> Result<RefId> {
        let ref_id = RefId::new();
        let artifact = StoredArtifact {
            data,
            content_type: content_type.to_string(),
            created_at: Self::current_timestamp(),
        };

        tracing::trace!(
            ref_id = %ref_id,
            size = artifact.data.len(),
            content_type = content_type,
            "Storing artifact in memory"
        );

        self.data.insert(ref_id.0.clone(), artifact);
        Ok(ref_id)
    }

    async fn load(&self, id: &RefId) -> Result<Option<Bytes>> {
        Ok(self.data.get(&id.0).map(|r| r.data.clone()))
    }

    async fn delete(&self, id: &RefId) -> Result<()> {
        self.data.remove(&id.0);
        Ok(())
    }

    async fn exists(&self, id: &RefId) -> Result<bool> {
        Ok(self.data.contains_key(&id.0))
    }

    async fn metadata(&self, id: &RefId) -> Result<Option<ArtifactMetadata>> {
        Ok(self.data.get(&id.0).map(|r| ArtifactMetadata {
            size: r.data.len(),
            content_type: r.content_type.clone(),
            created_at: r.created_at,
            tier: StorageTier::Hot,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_save_and_load() {
        let store = InMemoryStore::new();
        
        let data = Bytes::from("Hello, World!");
        let ref_id = store.save(data.clone()).await.unwrap();
        
        let loaded = store.load(&ref_id).await.unwrap();
        assert_eq!(loaded, Some(data));
    }

    #[tokio::test]
    async fn test_save_with_type() {
        let store = InMemoryStore::new();
        
        let data = Bytes::from("{\"key\": \"value\"}");
        let ref_id = store.save_with_type(data.clone(), "application/json").await.unwrap();
        
        let meta = store.metadata(&ref_id).await.unwrap().unwrap();
        assert_eq!(meta.content_type, "application/json");
        assert_eq!(meta.size, data.len());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryStore::new();
        
        let data = Bytes::from("To be deleted");
        let ref_id = store.save(data).await.unwrap();
        
        assert!(store.exists(&ref_id).await.unwrap());
        
        store.delete(&ref_id).await.unwrap();
        
        assert!(!store.exists(&ref_id).await.unwrap());
        assert!(store.load(&ref_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_not_found() {
        let store = InMemoryStore::new();
        let fake_id = RefId::from_string("nonexistent");
        
        assert!(!store.exists(&fake_id).await.unwrap());
        assert!(store.load(&fake_id).await.unwrap().is_none());
        assert!(store.metadata(&fake_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_memory_usage() {
        let store = InMemoryStore::new();
        
        let data1 = Bytes::from("Hello");
        let data2 = Bytes::from("World!");
        
        store.save(data1.clone()).await.unwrap();
        store.save(data2.clone()).await.unwrap();
        
        assert_eq!(store.len(), 2);
        assert_eq!(store.memory_usage(), data1.len() + data2.len());
    }
}
