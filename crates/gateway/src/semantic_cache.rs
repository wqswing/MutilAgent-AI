//! Semantic cache for high-frequency queries.

use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use multi_agent_core::{
    traits::{LlmClient, SemanticCache},
    Result,
};

/// Cache entry with expiration and embedding.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Cached response.
    response: String,
    /// Embedding vector of the query.
    query_embedding: Option<Vec<f32>>,
    /// When the entry was created.
    created_at: Instant,
    /// Time-to-live.
    ttl: Duration,
    /// Hit count for analytics.
    hit_count: u64,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// In-memory semantic cache.
///
/// Uses exact string matching for speed, falling back to
/// cosine similarity search using vector embeddings.
pub struct InMemorySemanticCache {
    /// Cache storage.
    cache: DashMap<String, CacheEntry>,
    /// LLM client for generating embeddings.
    llm_client: Arc<dyn LlmClient>,
    /// Similarity threshold (0.0 - 1.0).
    threshold: f32,
    /// Default TTL.
    default_ttl: Duration,
}

impl InMemorySemanticCache {
    /// Create a new semantic cache.
    pub fn new(llm_client: Arc<dyn LlmClient>) -> Self {
        Self {
            cache: DashMap::new(),
            llm_client,
            threshold: 0.90, // Higher threshold for semantic match
            default_ttl: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Set the similarity threshold.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set the default TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Normalize a query for caching.
    fn normalize_query(&self, query: &str) -> String {
        query
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Calculate cosine similarity between two vectors.
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        let total_entries = self.cache.len();
        let total_hits: u64 = self.cache.iter().map(|r| r.hit_count).sum();

        CacheStats {
            total_entries,
            total_hits,
        }
    }

    /// Clear expired entries.
    pub fn cleanup(&self) {
        self.cache.retain(|_: &String, v: &mut CacheEntry| !v.is_expired());
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total number of entries.
    pub total_entries: usize,
    /// Total number of cache hits.
    pub total_hits: u64,
}

#[async_trait]
impl SemanticCache for InMemorySemanticCache {
    async fn get(&self, query: &str) -> Result<Option<String>> {
        let normalized = self.normalize_query(query);

        // 1. Exact match (Fast path)
        if let Some(mut entry) = self.cache.get_mut(&normalized) {
            if !entry.is_expired() {
                entry.hit_count += 1;
                tracing::debug!(query = query, "Semantic cache exact hit");
                return Ok(Some(entry.response.clone()));
            }
        }

        // 2. Semantic match (Slow path)
        // Generate embedding for the query
        let query_embedding = match self.llm_client.embed(query).await {
            Ok(emb) => emb,
            Err(e) => {
                tracing::warn!("Failed to generate embedding for cache query: {}", e);
                return Ok(None);
            }
        };

        // Iterate and find best match
        let mut best_match: Option<String> = None;
        let mut max_similarity = 0.0;

        for entry in self.cache.iter() {
             if entry.is_expired() {
                 continue;
             }
             
             if let Some(ref stored_embedding) = entry.value().query_embedding {
                 let sim = self.cosine_similarity(&query_embedding, stored_embedding);
                 if sim > max_similarity {
                     max_similarity = sim;
                     if sim >= self.threshold {
                         best_match = Some(entry.value().response.clone());
                     }
                 }
             }
        }

        if let Some(response) = best_match {
            tracing::debug!(
                query = query,
                similarity = max_similarity,
                "Semantic cache fuzzy hit"
            );
            return Ok(Some(response));
        }

        tracing::debug!(query = query, "Semantic cache miss");
        Ok(None)
    }

    async fn set(&self, query: &str, response: &str) -> Result<()> {
        let normalized = self.normalize_query(query);

        let query_embedding = match self.llm_client.embed(query).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!("Failed to generate embedding for cache set: {}", e);
                None
            }
        };

        let entry = CacheEntry {
            response: response.to_string(),
            query_embedding,
            created_at: Instant::now(),
            ttl: self.default_ttl,
            hit_count: 0,
        };

        tracing::debug!(
            query = query,
            response_len = response.len(),
            has_embedding = entry.query_embedding.is_some(),
            "Caching response"
        );

        self.cache.insert(normalized, entry);
        Ok(())
    }

    async fn invalidate(&self, pattern: &str) -> Result<()> {
        let pattern_lower = pattern.to_lowercase();
        self.cache.retain(|key: &String, _: &mut CacheEntry| !key.contains(&pattern_lower));
        tracing::debug!(pattern = pattern, "Invalidated cache entries");
        Ok(())
    }
}

// Mock LlmClient for testing
#[cfg(test)]
mod tests {
    use super::*;
    use multi_agent_core::traits::{ChatMessage, LlmResponse};
    
    struct MockLlm;
    #[async_trait]
    impl LlmClient for MockLlm {
        async fn complete(&self, _prompt: &str) -> Result<LlmResponse> { unimplemented!() }
        async fn chat(&self, _messages: &[ChatMessage]) -> Result<LlmResponse> { unimplemented!() }
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            // Simple mock: hash length to float vec
             Ok(vec![text.len() as f32])
        }
    }

    #[tokio::test]
    async fn test_exact_match() {
        let client = Arc::new(MockLlm);
        let cache = InMemorySemanticCache::new(client);

        cache.set("Rust", "Language").await.unwrap();
        let hit = cache.get("Rust").await.unwrap();
        assert_eq!(hit, Some("Language".to_string()));
    }
}
