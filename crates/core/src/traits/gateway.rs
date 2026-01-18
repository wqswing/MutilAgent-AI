//! L0 Gateway traits.

use async_trait::async_trait;
use crate::error::Result;
use crate::types::{NormalizedRequest, UserIntent};

/// Intent router for classifying incoming requests.
#[async_trait]
pub trait IntentRouter: Send + Sync {
    /// Classify the intent of a normalized request.
    async fn classify(&self, request: &NormalizedRequest) -> Result<UserIntent>;
}

/// Semantic cache for high-frequency queries.
#[async_trait]
pub trait SemanticCache: Send + Sync {
    /// Check if a similar query exists in the cache.
    /// Returns the cached response if similarity > threshold.
    async fn get(&self, query: &str) -> Result<Option<String>>;

    /// Store a query-response pair in the cache.
    async fn set(&self, query: &str, response: &str) -> Result<()>;

    /// Invalidate cache entries matching a pattern.
    async fn invalidate(&self, pattern: &str) -> Result<()>;
}
