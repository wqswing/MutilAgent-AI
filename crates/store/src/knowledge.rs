//! In-memory Knowledge Store implementation.
//!
//! Uses cosine similarity for semantic search. Suitable for development
//! and small-scale deployments. For production, swap with a SQLite-vec
//! or Qdrant-backed implementation.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use multi_agent_core::{
    traits::{KnowledgeStore, KnowledgeEntry},
    Result,
};

/// In-memory knowledge store with cosine similarity search.
pub struct InMemoryKnowledgeStore {
    entries: Arc<RwLock<Vec<KnowledgeEntry>>>,
}

impl InMemoryKnowledgeStore {
    /// Create a new empty knowledge store.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for InMemoryKnowledgeStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[async_trait]
impl KnowledgeStore for InMemoryKnowledgeStore {
    async fn store(&self, entry: KnowledgeEntry) -> Result<String> {
        let id = entry.id.clone();
        let mut entries = self.entries.write().await;
        // Upsert: replace if same ID exists
        entries.retain(|e| e.id != id);
        entries.push(entry);
        tracing::debug!(id = %id, total = entries.len(), "Knowledge entry stored");
        Ok(id)
    }

    async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<KnowledgeEntry>> {
        let entries = self.entries.read().await;

        let mut scored: Vec<(f32, &KnowledgeEntry)> = entries
            .iter()
            .map(|e| (cosine_similarity(query_embedding, &e.embedding), e))
            .collect();

        // Sort by similarity descending
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored
            .into_iter()
            .take(limit)
            .filter(|(score, _)| *score > 0.0) // Filter out zero-similarity results
            .map(|(_, entry)| entry.clone())
            .collect())
    }

    async fn search_by_tags(&self, tags: &[String], limit: usize) -> Result<Vec<KnowledgeEntry>> {
        let entries = self.entries.read().await;

        let results: Vec<KnowledgeEntry> = entries
            .iter()
            .filter(|e| tags.iter().any(|tag| e.tags.contains(tag)))
            .take(limit)
            .cloned()
            .collect();

        Ok(results)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.retain(|e| e.id != id);
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.entries.read().await.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, summary: &str, embedding: Vec<f32>, tags: Vec<&str>) -> KnowledgeEntry {
        KnowledgeEntry {
            id: id.to_string(),
            summary: summary.to_string(),
            source_task: "test task".to_string(),
            session_id: "session-1".to_string(),
            embedding,
            tags: tags.into_iter().map(String::from).collect(),
            created_at: 1000,
        }
    }

    #[tokio::test]
    async fn test_store_and_count() {
        let store = InMemoryKnowledgeStore::new();
        assert_eq!(store.count().await.unwrap(), 0);

        store.store(make_entry("k1", "Rust is fast", vec![1.0, 0.0, 0.0], vec!["lang"])).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        store.store(make_entry("k2", "Python is flexible", vec![0.0, 1.0, 0.0], vec!["lang"])).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_upsert() {
        let store = InMemoryKnowledgeStore::new();

        store.store(make_entry("k1", "v1", vec![1.0], vec![])).await.unwrap();
        store.store(make_entry("k1", "v2", vec![1.0], vec![])).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        // The updated entry should have v2
        let results = store.search(&[1.0], 10).await.unwrap();
        assert_eq!(results[0].summary, "v2");
    }

    #[tokio::test]
    async fn test_semantic_search() {
        let store = InMemoryKnowledgeStore::new();

        // Create entries with orthogonal embeddings
        store.store(make_entry("k1", "Rust", vec![1.0, 0.0, 0.0], vec![])).await.unwrap();
        store.store(make_entry("k2", "Python", vec![0.0, 1.0, 0.0], vec![])).await.unwrap();
        store.store(make_entry("k3", "Go", vec![0.0, 0.0, 1.0], vec![])).await.unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 1); // Only k1 should match (others have 0 similarity)
        assert_eq!(results[0].id, "k1");

        // A mixed query should find the closest
        let results = store.search(&[0.7, 0.7, 0.0], 3).await.unwrap();
        assert_eq!(results.len(), 2);
        // Both k1 and k2 should match, k3 has 0 similarity
    }

    #[tokio::test]
    async fn test_tag_search() {
        let store = InMemoryKnowledgeStore::new();

        store.store(make_entry("k1", "Rust", vec![1.0], vec!["systems", "fast"])).await.unwrap();
        store.store(make_entry("k2", "Python", vec![0.0], vec!["scripting", "fast"])).await.unwrap();
        store.store(make_entry("k3", "SQL", vec![0.0], vec!["database"])).await.unwrap();

        let results = store.search_by_tags(&["fast".to_string()], 10).await.unwrap();
        assert_eq!(results.len(), 2);

        let results = store.search_by_tags(&["database".to_string()], 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "k3");
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryKnowledgeStore::new();

        store.store(make_entry("k1", "v1", vec![1.0], vec![])).await.unwrap();
        store.store(make_entry("k2", "v2", vec![1.0], vec![])).await.unwrap();

        store.delete("k1").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        // Deleting non-existent ID should not error
        store.delete("nonexistent").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);
    }
}
