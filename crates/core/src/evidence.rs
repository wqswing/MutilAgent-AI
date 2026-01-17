//! Evidence Chain Store (L3 Layer)
//! 
//! Provides tamper-proof storage for agent outputs using SHA256 content hashing.
//! Implements Pass-by-Reference pattern where tools return Evidence IDs instead of raw data.

use std::collections::HashMap;
use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A tamper-proof evidence record with content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Unique reference ID (e.g., "ev_a1b2c3d4").
    pub id: String,
    /// SHA256 hash of the content for integrity verification.
    pub content_hash: String,
    /// Storage URL/path where the content is persisted.
    pub storage_url: String,
    /// Name of the tool or agent that created this evidence.
    pub created_by: String,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Evidence {
    /// Create a new Evidence record by sealing content.
    /// 
    /// # Arguments
    /// * `content` - Raw content bytes to seal.
    /// * `created_by` - Name of the creator (tool/agent).
    /// * `storage_url` - Where the content will be stored.
    pub fn seal(content: &[u8], created_by: impl Into<String>, storage_url: impl Into<String>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash_bytes = hasher.finalize();
        let content_hash = format!("{:x}", hash_bytes);
        
        let id = format!("ev_{}", Uuid::new_v4().to_string().split('-').next().unwrap());
        
        Self {
            id,
            content_hash,
            storage_url: storage_url.into(),
            created_by: created_by.into(),
            metadata: HashMap::new(),
        }
    }
    
    /// Verify that the given content matches this evidence's hash.
    pub fn verify(&self, content: &[u8]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash_bytes = hasher.finalize();
        let computed_hash = format!("{:x}", hash_bytes);
        
        self.content_hash == computed_hash
    }
    
    /// Add metadata to the evidence.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
    
    /// Format as a tool response string (Pass-by-Reference).
    pub fn to_tool_response(&self) -> String {
        format!(
            "Operation successful. Evidence ID: {}. Content Hash: {}",
            self.id, self.content_hash
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_evidence_seal() {
        let content = b"Hello, World!";
        let evidence = Evidence::seal(content, "test_tool", "s3://bucket/key");
        
        assert!(evidence.id.starts_with("ev_"));
        assert_eq!(evidence.content_hash.len(), 64); // SHA256 hex length
        assert_eq!(evidence.created_by, "test_tool");
        assert_eq!(evidence.storage_url, "s3://bucket/key");
    }
    
    #[test]
    fn test_evidence_verify_success() {
        let content = b"Test data";
        let evidence = Evidence::seal(content, "tool", "url");
        
        assert!(evidence.verify(content));
    }
    
    #[test]
    fn test_evidence_verify_failure() {
        let content = b"Original data";
        let evidence = Evidence::seal(content, "tool", "url");
        
        let tampered = b"Tampered data";
        assert!(!evidence.verify(tampered));
    }
    
    #[test]
    fn test_evidence_tool_response() {
        let evidence = Evidence::seal(b"data", "my_tool", "s3://x");
        let response = evidence.to_tool_response();
        
        assert!(response.contains("Evidence ID: ev_"));
        assert!(response.contains("Content Hash:"));
    }
    
    #[test]
    fn test_evidence_metadata() {
        let evidence = Evidence::seal(b"data", "tool", "url")
            .with_metadata("step", "1")
            .with_metadata("session", "abc123");
        
        assert_eq!(evidence.metadata.get("step"), Some(&"1".to_string()));
        assert_eq!(evidence.metadata.get("session"), Some(&"abc123".to_string()));
    }
}
