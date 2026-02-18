//! Audit logging for compliance and observability.

use async_trait::async_trait;
use multi_agent_core::{traits::Erasable, Result};
use serde::{Deserialize, Serialize};

/// Outcome of an audited action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOutcome {
    Success,
    Denied,
    Error(String),
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique entry ID.
    pub id: String,
    /// Timestamp (ISO 8601).
    pub timestamp: String,
    /// User identifier from RBAC.
    pub user_id: String,
    /// Action performed (e.g., "execute_tool", "update_config").
    pub action: String,
    /// Resource affected (e.g., tool name, config key).
    pub resource: String,
    /// Outcome of the action.
    pub outcome: AuditOutcome,
    /// Optional metadata (JSON).
    pub metadata: Option<serde_json::Value>,
    /// Secure link to previous entry (SHA-256 hash).
    pub previous_hash: Option<String>,
    /// Hash of current entry + previous_hash.
    pub hash: Option<String>,
}

/// Filter for querying audit logs.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub from_timestamp: Option<String>,
    pub to_timestamp: Option<String>,
    pub limit: Option<usize>,
}

/// Trait for audit log persistence.
#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Log an audit entry.
    async fn log(&self, entry: AuditEntry) -> Result<()>;

    /// Query audit logs with optional filters.
    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEntry>>;
}

/// In-memory audit store for testing.
pub struct InMemoryAuditStore {
    entries: std::sync::Mutex<Vec<AuditEntry>>,
}

impl InMemoryAuditStore {
    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl Default for InMemoryAuditStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuditStore for InMemoryAuditStore {
    async fn log(&self, entry: AuditEntry) -> Result<()> {
        self.entries.lock().unwrap().push(entry);
        Ok(())
    }

    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEntry>> {
        let entries = self.entries.lock().unwrap();
        let mut result: Vec<AuditEntry> = entries
            .iter()
            .filter(|e| {
                filter.user_id.as_ref().is_none_or(|u| &e.user_id == u)
                    && filter.action.as_ref().is_none_or(|a| &e.action == a)
                    && filter.resource.as_ref().is_none_or(|r| &e.resource == r)
            })
            .cloned()
            .collect();

        if let Some(limit) = filter.limit {
            result.truncate(limit);
        }

        Ok(result)
    }
}

#[async_trait]
impl Erasable for InMemoryAuditStore {
    async fn erase_user(&self, user_id: &str) -> Result<usize> {
        let mut entries = self.entries.lock().unwrap();
        let initial_len = entries.len();
        entries.retain(|e| e.user_id != user_id);
        Ok(initial_len - entries.len())
    }
}

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

/// Secure audit store using SQLite and Hash Chaining.
pub struct SqliteAuditStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteAuditStore {
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| multi_agent_core::error::Error::Governance(format!("DB error: {}", e)))?;

        // Initialize schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS audit_logs (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                user_id TEXT NOT NULL,
                action TEXT NOT NULL,
                resource TEXT NOT NULL,
                outcome TEXT NOT NULL,
                metadata TEXT,
                previous_hash TEXT,
                hash TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| multi_agent_core::error::Error::Governance(format!("Schema error: {}", e)))?;

        // Index for performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_user ON audit_logs (user_id)",
            [],
        )
        .map_err(|e| multi_agent_core::error::Error::Governance(format!("Index error: {}", e)))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn calculate_hash(entry: &AuditEntry, prev_hash: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&entry.id);
        hasher.update(&entry.timestamp);
        hasher.update(&entry.user_id);
        hasher.update(&entry.action);
        hasher.update(&entry.resource);
        hasher.update(serde_json::to_string(&entry.outcome).unwrap_or_default());
        hasher.update(
            entry
                .metadata
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_default(),
        );
        if let Some(ph) = prev_hash {
            hasher.update(ph);
        }
        format!("{:x}", hasher.finalize())
    }
}

#[async_trait]
impl AuditStore for SqliteAuditStore {
    async fn log(&self, mut entry: AuditEntry) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().unwrap();
            let tx = conn.transaction()
                .map_err(|e| multi_agent_core::error::Error::Governance(format!("Tx error: {}", e)))?;

            // Get previous hash
            let prev_hash: Option<String> = tx.query_row(
                "SELECT hash FROM audit_logs ORDER BY timestamp DESC, rowid DESC LIMIT 1",
                [],
                |row| row.get(0),
            ).optional()
            .map_err(|e| multi_agent_core::error::Error::Governance(format!("Query error: {}", e)))?;

            entry.previous_hash = prev_hash.clone();
            entry.hash = Some(Self::calculate_hash(&entry, prev_hash.as_deref()));

            tx.execute(
                "INSERT INTO audit_logs (id, timestamp, user_id, action, resource, outcome, metadata, previous_hash, hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.id,
                    entry.timestamp,
                    entry.user_id,
                    entry.action,
                    entry.resource,
                    serde_json::to_string(&entry.outcome).unwrap_or_default(),
                    entry.metadata.map(|m| m.to_string()),
                    entry.previous_hash,
                    entry.hash
                ],
            ).map_err(|e| multi_agent_core::error::Error::Governance(format!("Insert error: {}", e)))?;

            tx.commit()
                .map_err(|e| multi_agent_core::error::Error::Governance(format!("Commit error: {}", e)))?;
            Ok(())
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }

    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEntry>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut query = "SELECT id, timestamp, user_id, action, resource, outcome, metadata, previous_hash, hash FROM audit_logs WHERE 1=1".to_string();
            let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(uid) = &filter.user_id {
                query.push_str(" AND user_id = ?");
                params_vec.push(Box::new(uid.clone()));
            }
            if let Some(act) = &filter.action {
                query.push_str(" AND action = ?");
                params_vec.push(Box::new(act.clone()));
            }
            if let Some(res) = &filter.resource {
                query.push_str(" AND resource = ?");
                params_vec.push(Box::new(res.clone()));
            }

            query.push_str(" ORDER BY timestamp DESC");
            if let Some(limit) = filter.limit {
                query.push_str(&format!(" LIMIT {}", limit));
            }

            let mut stmt = conn.prepare(&query)
                .map_err(|e| multi_agent_core::error::Error::Governance(format!("Prepare error: {}", e)))?;

            let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

            let entries = stmt.query_map(&param_refs[..], |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    user_id: row.get(2)?,
                    action: row.get(3)?,
                    resource: row.get(4)?,
                    outcome: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(AuditOutcome::Success),
                    metadata: row.get::<_, Option<String>>(6)?.and_then(|m| serde_json::from_str(&m).ok()),
                    previous_hash: row.get(7)?,
                    hash: row.get(8)?,
                })
            }).map_err(|e| multi_agent_core::error::Error::Governance(format!("Query error: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| multi_agent_core::error::Error::Governance(format!("Result error: {}", e)))?;

            Ok(entries)
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }
}

#[async_trait]
impl Erasable for SqliteAuditStore {
    async fn erase_user(&self, user_id: &str) -> Result<usize> {
        let conn = self.conn.clone();
        let uid = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let count = conn
                .execute("DELETE FROM audit_logs WHERE user_id = ?", params![uid])
                .map_err(|e| {
                    multi_agent_core::error::Error::Governance(format!("Delete error: {}", e))
                })?;
            Ok(count)
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_audit_store_basics() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        let store = SqliteAuditStore::new(path).unwrap();

        let entry = AuditEntry {
            id: "test-1".into(),
            timestamp: "2023-01-01T00:00:00Z".into(),
            user_id: "user-1".into(),
            action: "TEST_ACTION".into(),
            resource: "res-1".into(),
            outcome: AuditOutcome::Success,
            metadata: None,
            previous_hash: None,
            hash: None,
        };

        store.log(entry.clone()).await.unwrap();

        let filter = AuditFilter {
            user_id: Some("user-1".into()),
            ..Default::default()
        };
        let results = store.query(filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "test-1");
        assert!(results[0].hash.is_some());
    }

    #[tokio::test]
    async fn test_hash_chain_integrity() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        let store = SqliteAuditStore::new(path).unwrap();

        let entry1 = AuditEntry {
            id: "test-1".into(),
            timestamp: "2023-01-01T00:00:00Z".into(),
            user_id: "user-1".into(),
            action: "ACTION_1".into(),
            resource: "res-1".into(),
            outcome: AuditOutcome::Success,
            metadata: None,
            previous_hash: None,
            hash: None,
        };

        let entry2 = AuditEntry {
            id: "test-2".into(),
            timestamp: "2023-01-01T00:00:01Z".into(),
            user_id: "user-1".into(),
            action: "ACTION_2".into(),
            resource: "res-2".into(),
            outcome: AuditOutcome::Success,
            metadata: None,
            previous_hash: None,
            hash: None,
        };

        store.log(entry1).await.unwrap();
        store.log(entry2).await.unwrap();

        let results = store.query(AuditFilter::default()).await.unwrap();
        assert_eq!(results.len(), 2);

        // results is ordered by timestamp DESC
        let e2 = &results[0]; // test-2
        let e1 = &results[1]; // test-1

        assert_eq!(e2.previous_hash, e1.hash);

        // Verify e2 hash
        let expected_hash = SqliteAuditStore::calculate_hash(e2, e1.hash.as_deref());
        assert_eq!(e2.hash.as_deref(), Some(expected_hash.as_str()));
    }
}
