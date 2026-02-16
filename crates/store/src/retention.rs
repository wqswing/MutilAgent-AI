//! Data retention policy and pruning logic.

use std::time::Duration;
use async_trait::async_trait;
use crate::Result;
use serde::{Serialize, Deserialize};

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

/// Trait for stores that support deleting all data for a specific user (GDPR).
#[async_trait]
pub trait Erasable: Send + Sync {
    /// Delete all data associated with the given user ID.
    /// Returns the number of items deleted.
    async fn erase_user(&self, user_id: &str) -> Result<usize>;
}
