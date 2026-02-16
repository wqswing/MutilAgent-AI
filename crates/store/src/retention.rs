//! Data retention policy and pruning logic.

use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for data retention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Maximum age of artifacts/sessions (e.g., 30 days).
    pub max_age: Option<Duration>,
    /// Maximum number of items to keep (per user/tenant).
    pub max_items: Option<usize>,
    /// Maximum total size in bytes (per user/tenant).
    pub max_size_bytes: Option<u64>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age: Some(Duration::from_secs(30 * 24 * 60 * 60)), // 30 days
            max_items: None,
            max_size_bytes: None,
        }
    }
}

/// Trait for stores that support pruning old data.
#[async_trait]
pub trait Prunable: Send + Sync {
    /// Prune data older than the specified duration.
    /// Returns the number of items deleted.
    async fn prune(&self, max_age: Duration) -> Result<usize>;
}

pub use multi_agent_core::traits::Erasable;
