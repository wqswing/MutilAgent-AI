//! State store traits for stateless architecture.
//!
//! These traits enable horizontal scaling by externalizing runtime state
//! to external stores like Redis or PostgreSQL.

use async_trait::async_trait;
use std::time::Duration;
use crate::error::Result;

// =============================================================================
// Generic State Store
// =============================================================================

/// Generic key-value state store for externalizing runtime state.
///
/// Implementations can use Redis, PostgreSQL, or other backends.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Get a value by key.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    
    /// Set a value with optional TTL.
    async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<()>;
    
    /// Delete a key.
    async fn delete(&self, key: &str) -> Result<()>;
    
    /// Check if a key exists.
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.get(key).await?.is_some())
    }
    
    /// Set a value only if the key does not exist (for distributed locks).
    async fn set_nx(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<bool>;
}

// =============================================================================
// Distributed Rate Limiter
// =============================================================================

/// Distributed rate limiter for cross-instance request throttling.
///
/// Implementations should use atomic operations (e.g., Redis Lua scripts)
/// to ensure correctness under concurrent access.
#[async_trait]
pub trait DistributedRateLimiter: Send + Sync {
    /// Check if a request is allowed under the rate limit.
    ///
    /// Returns `true` if allowed, `false` if rate limited.
    /// The implementation should atomically increment the counter.
    async fn check_and_increment(&self, key: &str, limit: u32, window: Duration) -> Result<bool>;
    
    /// Get the remaining quota for a key.
    async fn remaining(&self, key: &str, limit: u32, window: Duration) -> Result<u32>;
    
    /// Reset the rate limit for a key (for admin/testing).
    async fn reset(&self, key: &str) -> Result<()>;
}

// =============================================================================
// Provider Store (for Admin)
// =============================================================================

/// Persistent storage for LLM provider configurations.
#[async_trait]
pub trait ProviderStore: Send + Sync {
    /// List all providers.
    async fn list(&self) -> Result<Vec<ProviderEntry>>;
    
    /// Get a provider by ID.
    async fn get(&self, id: &str) -> Result<Option<ProviderEntry>>;
    
    /// Add or update a provider.
    async fn upsert(&self, provider: &ProviderEntry) -> Result<()>;
    
    /// Delete a provider.
    async fn delete(&self, id: &str) -> Result<bool>;
}

use serde::{Serialize, Deserialize};

/// Provider entry for external storage.
/// Mirrors the admin crate's ProviderEntry but without serde skip attributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntry {

    pub id: String,
    pub vendor: String,
    pub model_id: String,
    pub description: Option<String>,
    pub base_url: String,
    pub version: Option<String>,
    /// Reference to encrypted API key in secrets manager.
    pub api_key_id: String,
    pub capabilities: Vec<String>,
    pub status: String,
}
