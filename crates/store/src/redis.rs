//! Redis implementation of SessionStore.

use async_trait::async_trait;
use redis::{Client, AsyncCommands, Script};
use std::time::Duration;

use multi_agent_core::{
    traits::{SessionStore, StateStore, DistributedRateLimiter, ProviderStore, ProviderEntry},
    types::Session,
    Error, Result,
};

// =============================================================================
// Redis Provider Store (for Admin)
// =============================================================================

/// Redis persistence for providers.
pub struct RedisProviderStore {
    client: Client,
    prefix: String,
}

impl RedisProviderStore {
    /// Create a new Redis provider store.
    pub fn new(url: &str, prefix: &str) -> Result<Self> {
        let client = Client::open(url)
            .map_err(|e| Error::storage(format!("Failed to connect to Redis: {}", e)))?;
        Ok(Self {
            client,
            prefix: prefix.to_string(),
        })
    }
}

#[async_trait]
impl ProviderStore for RedisProviderStore {
    async fn list(&self) -> Result<Vec<ProviderEntry>> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
            
        // Scan for all provider keys
        let pattern = format!("{}:*", self.prefix);
        let keys: Vec<String> = conn.keys(&pattern).await
            .map_err(|e| Error::storage(format!("Redis keys error: {}", e)))?;
            
        let mut providers = Vec::new();
        for key in keys {
            let data: Option<String> = conn.get(&key).await
                .map_err(|e| Error::storage(format!("Redis get error: {}", e)))?;
                
            if let Some(json) = data {
                if let Ok(provider) = serde_json::from_str::<ProviderEntry>(&json) {
                    providers.push(provider);
                }
            }
        }
        
        Ok(providers)
    }

    async fn get(&self, id: &str) -> Result<Option<ProviderEntry>> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
            
        let key = format!("{}:{}", self.prefix, id);
        let data: Option<String> = conn.get(&key).await
             .map_err(|e| Error::storage(format!("Redis get error: {}", e)))?;

        match data {
            Some(json) => {
                let provider = serde_json::from_str(&json)
                    .map_err(|e| Error::storage(format!("Failed to deserialize provider: {}", e)))?;
                Ok(Some(provider))
            },
            None => Ok(None),
        }
    }

    async fn upsert(&self, provider: &ProviderEntry) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;

        let key = format!("{}:{}", self.prefix, provider.id);
        let json = serde_json::to_string(provider)
            .map_err(|e| Error::storage(format!("Failed to serialize provider: {}", e)))?;

        let _: () = conn.set(&key, json).await
            .map_err(|e| Error::storage(format!("Redis set error: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
            
        let key = format!("{}:{}", self.prefix, id);
        let count: i32 = conn.del(&key).await
             .map_err(|e| Error::storage(format!("Redis delete error: {}", e)))?;
             
        Ok(count > 0)
    }
}



// =============================================================================
// Redis Session Store (existing implementation)
// =============================================================================

/// Redis persistence for sessions.
pub struct RedisSessionStore {
    client: Client,
    prefix: String,
    ttl_seconds: usize,
}

impl RedisSessionStore {
    /// Create a new Redis session store.
    pub fn new(url: &str, prefix: &str, ttl_seconds: usize) -> Result<Self> {
        let client = Client::open(url)
            .map_err(|e| Error::storage(format!("Failed to connect to Redis: {}", e)))?;
        
        Ok(Self {
            client,
            prefix: prefix.to_string(),
            ttl_seconds,
        })
    }

    fn key(&self, id: &str) -> String {
        format!("{}:{}", self.prefix, id)
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn load(&self, id: &str) -> Result<Option<Session>> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
            
        let key = self.key(id);
        let data: Option<String> = conn.get(&key).await
            .map_err(|e| Error::storage(format!("Redis get error: {}", e)))?;

        match data {
            Some(json) => {
                let session = serde_json::from_str(&json)
                    .map_err(|e| Error::storage(format!("Failed to deserialize session: {}", e)))?;
                Ok(Some(session))
            },
            None => Ok(None),
        }
    }

    async fn save(&self, session: &Session) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;

        let key = self.key(&session.id);
        let json = serde_json::to_string(session)
            .map_err(|e| Error::storage(format!("Failed to serialize session: {}", e)))?;

        // Set with TTL
        let _: () = conn.set_ex(&key, json, self.ttl_seconds as u64).await
            .map_err(|e| Error::storage(format!("Redis set error: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
            
        let key = self.key(id);
        let _: () = conn.del(&key).await
             .map_err(|e| Error::storage(format!("Redis delete error: {}", e)))?;
             
        Ok(())
    }

    async fn list_running(&self) -> Result<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
            
        let pattern = format!("{}*", self.prefix);
        let keys: Vec<String> = conn.keys(&pattern).await
            .map_err(|e| Error::storage(format!("Redis keys error: {}", e)))?;
            
        let mut running_ids = Vec::new();
        for key in keys {
            let data: Option<String> = conn.get(&key).await
                .map_err(|e| Error::storage(format!("Redis get error: {}", e)))?;
                
            if let Some(json) = data {
                if let Ok(session) = serde_json::from_str::<Session>(&json) {
                    if session.status == multi_agent_core::types::SessionStatus::Running {
                        running_ids.push(session.id);
                    }
                }
            }
        }
        
        Ok(running_ids)
    }
}

// =============================================================================
// Redis State Store (generic key-value for stateless architecture)
// =============================================================================

/// Generic Redis state store implementing StateStore trait.
pub struct RedisStateStore {
    client: Client,
}

impl RedisStateStore {
    /// Create a new Redis state store.
    pub fn new(url: &str) -> Result<Self> {
        let client = Client::open(url)
            .map_err(|e| Error::storage(format!("Failed to connect to Redis: {}", e)))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl StateStore for RedisStateStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
        let data: Option<Vec<u8>> = conn.get(key).await
            .map_err(|e| Error::storage(format!("Redis get error: {}", e)))?;
        Ok(data)
    }

    async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
        
        if let Some(ttl) = ttl {
            let _: () = conn.set_ex(key, value, ttl.as_secs()).await
                .map_err(|e| Error::storage(format!("Redis set error: {}", e)))?;
        } else {
            let _: () = conn.set(key, value).await
                .map_err(|e| Error::storage(format!("Redis set error: {}", e)))?;
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
        let _: () = conn.del(key).await
            .map_err(|e| Error::storage(format!("Redis delete error: {}", e)))?;
        Ok(())
    }

    async fn set_nx(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
        
        // SETNX returns true if key was set (did not exist)
        let result: bool = conn.set_nx(key, value).await
            .map_err(|e| Error::storage(format!("Redis setnx error: {}", e)))?;
        
        if result {
            if let Some(ttl) = ttl {
                let _: () = conn.expire(key, ttl.as_secs() as i64).await
                    .map_err(|e| Error::storage(format!("Redis expire error: {}", e)))?;
            }
        }
        Ok(result)
    }
}

// =============================================================================
// Redis Rate Limiter (sliding window using Lua script)
// =============================================================================

/// Distributed rate limiter using Redis sorted sets and Lua scripting.
pub struct RedisRateLimiter {
    client: Client,
    script: Script,
}

impl RedisRateLimiter {
    /// Create a new Redis rate limiter.
    pub fn new(url: &str) -> Result<Self> {
        let client = Client::open(url)
            .map_err(|e| Error::storage(format!("Failed to connect to Redis: {}", e)))?;
        
        // Sliding window rate limiter using sorted sets
        // KEYS[1] = rate limit key
        // ARGV[1] = current timestamp (ms)
        // ARGV[2] = window size (ms)
        // ARGV[3] = limit
        // ARGV[4] = unique request ID
        let lua_script = r#"
            local key = KEYS[1]
            local now = tonumber(ARGV[1])
            local window = tonumber(ARGV[2])
            local limit = tonumber(ARGV[3])
            local request_id = ARGV[4]
            
            -- Remove expired entries
            redis.call('ZREMRANGEBYSCORE', key, 0, now - window)
            
            -- Count current requests in window
            local count = redis.call('ZCARD', key)
            
            if count < limit then
                -- Add new request
                redis.call('ZADD', key, now, request_id)
                redis.call('PEXPIRE', key, window)
                return 1  -- Allowed
            else
                return 0  -- Rate limited
            end
        "#;
        
        Ok(Self {
            client,
            script: Script::new(lua_script),
        })
    }
}

#[async_trait]
impl DistributedRateLimiter for RedisRateLimiter {
    async fn check_and_increment(&self, key: &str, limit: u32, window: Duration) -> Result<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let window_ms = window.as_millis() as u64;
        let request_id = format!("{}:{}", now, rand::random::<u64>());
        
        let result: i32 = self.script
            .key(key)
            .arg(now)
            .arg(window_ms)
            .arg(limit)
            .arg(&request_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| Error::storage(format!("Redis script error: {}", e)))?;
        
        Ok(result == 1)
    }

    async fn remaining(&self, key: &str, limit: u32, window: Duration) -> Result<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let window_ms = window.as_millis() as u64;
        
        // Remove expired and count
        let _: () = conn.zrembyscore(key, 0i64, (now - window_ms) as i64).await
            .map_err(|e| Error::storage(format!("Redis zremrangebyscore error: {}", e)))?;
        let count: u32 = conn.zcard(key).await
            .map_err(|e| Error::storage(format!("Redis zcard error: {}", e)))?;
        
        Ok(limit.saturating_sub(count))
    }

    async fn reset(&self, key: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| Error::storage(format!("Redis connection error: {}", e)))?;
        let _: () = conn.del(key).await
            .map_err(|e| Error::storage(format!("Redis delete error: {}", e)))?;
        Ok(())
    }
}

