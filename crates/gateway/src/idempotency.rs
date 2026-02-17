use dashmap::DashMap;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub request_hash: String,
    pub status: u16,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum IdempotencyLookup {
    Miss,
    Replay(IdempotencyRecord),
    Conflict,
}

#[derive(Default)]
pub struct IdempotencyStore {
    records: DashMap<String, IdempotencyRecord>,
}

impl IdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn scope_key(scope: &str, key: &str) -> String {
        format!("{}::{}", scope, key)
    }

    pub fn hash_payload(value: &serde_json::Value) -> String {
        let bytes = serde_json::to_vec(value).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    pub fn check(&self, scope: &str, key: &str, request_hash: &str) -> IdempotencyLookup {
        let composite = Self::scope_key(scope, key);
        if let Some(existing) = self.records.get(&composite) {
            if existing.request_hash == request_hash {
                IdempotencyLookup::Replay(existing.clone())
            } else {
                IdempotencyLookup::Conflict
            }
        } else {
            IdempotencyLookup::Miss
        }
    }

    pub fn store(
        &self,
        scope: &str,
        key: &str,
        request_hash: String,
        status: u16,
        body: serde_json::Value,
    ) {
        let composite = Self::scope_key(scope, key);
        self.records.insert(
            composite,
            IdempotencyRecord {
                request_hash,
                status,
                body,
            },
        );
    }
}

