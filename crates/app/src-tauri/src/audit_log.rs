use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::fs::{OpenOptions, File};
use std::io::{Write, BufReader, BufRead};
use std::path::{Path, PathBuf};
use multi_agent_core::events::EventEnvelope;
use anyhow::{Result, Context};

/// An entry in the tamper-evident audit log.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    /// The original event envelope.
    pub envelope: EventEnvelope,
    /// Hash of the previous entry.
    pub prev_hash: String,
    /// Hash of this entry (including prev_hash and envelope).
    pub hash: String,
}

/// A tamper-evident audit log that writes to a JSONL file.
pub struct AuditLog {
    file_path: PathBuf,
    last_hash: String,
}

impl AuditLog {
    /// Create or open an audit log at the given path.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let file_path = path.into();
        let mut last_hash = "0".repeat(64); // Genesis hash

        if file_path.exists() {
            // Recover last hash by reading the last line
            let file = File::open(&file_path)?;
            let reader = BufReader::new(file);
            if let Some(last_line) = reader.lines().last() {
                let line = last_line?;
                let entry: AuditEntry = serde_json::from_str(&line)
                    .context("Failed to parse last audit entry")?;
                last_hash = entry.hash;
            }
        }

        Ok(Self {
            file_path,
            last_hash,
        })
    }

    /// Append a new event to the audit log.
    pub fn append(&mut self, envelope: EventEnvelope) -> Result<()> {
        let prev_hash = self.last_hash.clone();
        
        // Calculate new hash
        let mut hasher = Sha256::new();
        hasher.update(prev_hash.as_bytes());
        let envelope_json = serde_json::to_string(&envelope)?;
        hasher.update(envelope_json.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        let entry = AuditEntry {
            envelope,
            prev_hash,
            hash: hash.clone(),
        };

        // Write to file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        
        let line = serde_json::to_string(&entry)?;
        writeln!(file, "{}", line)?;

        self.last_hash = hash;
        Ok(())
    }
}

/// An event subscriber that writes to the audit log.
pub struct AuditSubscriber {
    log: std::sync::Arc<tokio::sync::Mutex<AuditLog>>,
}

impl AuditSubscriber {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let log = AuditLog::open(path)?;
        Ok(Self {
            log: std::sync::Arc::new(tokio::sync::Mutex::new(log)),
        })
    }
}

#[async_trait::async_trait]
impl multi_agent_core::traits::EventEmitter for AuditSubscriber {
    async fn emit(&self, event: EventEnvelope) {
        // Filter events for auditing
        let should_audit = matches!(
            event.event_type.as_str(),
            "TOOL_EXEC_FINISHED" | "APPROVAL_DECIDED" | "POLICY_EVALUATED" | "FS_WRITE" | "FS_READ"
        );

        if should_audit {
            let mut log = self.log.lock().await;
            if let Err(e) = log.append(event) {
                tracing::error!("Failed to record audit entry: {}", e);
            }
        }
    }
}
