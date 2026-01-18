//! In-memory vector store implementation.
//! 
//! This module provides a simple, in-memory vector database using cosine similarity.
//! It serves as a reference implementation and fallback for the memory system.

use async_trait::async_trait;
use dashmap::DashMap;
use multi_agent_core::{
    traits::{MemoryStore, MemoryEntry},
    Result,
};

/// Simple in-memory vector store.
#[derive(Debug, Default)]
pub struct SimpleVectorStore {
    data: DashMap<String, MemoryEntry>,
}

impl SimpleVectorStore {
    /// Create a new simple vector store.
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
        }
    }

    /// Calculate cosine similarity between two vectors.
    fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
        if v1.len() != v2.len() {
            return 0.0;
        }

        let dot_product: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let magnitude1: f32 = v1.iter().map(|a| a * a).sum::<f32>().sqrt();
        let magnitude2: f32 = v2.iter().map(|a| a * a).sum::<f32>().sqrt();

        if magnitude1 == 0.0 || magnitude2 == 0.0 {
            return 0.0;
        }

        dot_product / (magnitude1 * magnitude2)
    }
}

#[async_trait]
impl MemoryStore for SimpleVectorStore {
    async fn add(&self, entry: MemoryEntry) -> Result<()> {
        self.data.insert(entry.id.clone(), entry);
        Ok(())
    }

    async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<MemoryEntry>> {
        let mut scored_entries: Vec<(f32, MemoryEntry)> = self
            .data
            .iter()
            .map(|entry| {
                let score = Self::cosine_similarity(query_embedding, &entry.value().embedding);
                (score, entry.value().clone())
            })
            .collect();

        // Sort by score descending
        scored_entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Take top 'limit'
        Ok(scored_entries
            .into_iter()
            .take(limit)
            .map(|(_, entry)| entry)
            .collect())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        self.data.remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vector_search() {
        let store = SimpleVectorStore::new();

        // Add dummy entries
        let e1 = MemoryEntry {
            id: "1".to_string(),
            content: "Apple".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata: Default::default(),
        };
        let e2 = MemoryEntry {
            id: "2".to_string(),
            content: "Banana".to_string(),
            embedding: vec![0.0, 1.0, 0.0],
            metadata: Default::default(),
        };
        
        store.add(e1).await.unwrap();
        store.add(e2).await.unwrap();

        // Search close to Apple
        let query = vec![0.9, 0.1, 0.0];
        let results = store.search(&query, 1).await.unwrap();
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Apple");
    }
}
