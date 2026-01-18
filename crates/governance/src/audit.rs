//! Audit logging for compliance and observability.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use multi_agent_core::Result;

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


use std::fs::OpenOptions;
use std::io::Write;

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
                filter.user_id.as_ref().map_or(true, |u| &e.user_id == u)
                    && filter.action.as_ref().map_or(true, |a| &e.action == a)
                    && filter.resource.as_ref().map_or(true, |r| &e.resource == r)
            })
            .cloned()
            .collect();
        
        if let Some(limit) = filter.limit {
            result.truncate(limit);
        }
        
        Ok(result)
    }
}

/// Persistent audit store using JSON lines.
pub struct FileAuditStore {
    file_path: std::path::PathBuf,
}

impl FileAuditStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { file_path: path.into() }
    }
}

#[async_trait]
impl AuditStore for FileAuditStore {
    async fn log(&self, entry: AuditEntry) -> Result<()> {
        let path = self.file_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| multi_agent_core::error::Error::Governance(format!("Audit file error: {}", e)))?;
            
            let json = serde_json::to_string(&entry).map_err(|e| multi_agent_core::error::Error::Serialization(e))?;
            writeln!(file, "{}", json).map_err(|e| multi_agent_core::error::Error::Governance(format!("Audit write error: {}", e)))?;
            Ok(())
        }).await.map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }
    
    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEntry>> {
        let path = self.file_path.clone();
        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(Vec::new());
            }
            
            let content = std::fs::read_to_string(path)
                .map_err(|e| multi_agent_core::error::Error::Governance(format!("Audit read error: {}", e)))?;
                
            let mut result: Vec<AuditEntry> = content.lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str::<AuditEntry>(line).ok())
                .filter(|e| {
                    filter.user_id.as_ref().map_or(true, |u| &e.user_id == u)
                        && filter.action.as_ref().map_or(true, |a| &e.action == a)
                        && filter.resource.as_ref().map_or(true, |r| &e.resource == r)
                })
                .collect();
                
            if let Some(limit) = filter.limit {
                result.truncate(limit);
            }
            Ok(result)
        }).await.map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }
}
