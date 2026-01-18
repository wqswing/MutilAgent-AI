//! Redis implementation of SessionStore.

use async_trait::async_trait;
use redis::{Client, AsyncCommands};

use mutil_agent_core::{
    traits::SessionStore,
    types::Session,
    Error, Result,
};

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
            // Optimization: We could fetch status only, but here we fetch whole session
            let data: Option<String> = conn.get(&key).await
                .map_err(|e| Error::storage(format!("Redis get error: {}", e)))?;
                
            if let Some(json) = data {
                if let Ok(session) = serde_json::from_str::<Session>(&json) {
                    if session.status == mutil_agent_core::types::SessionStatus::Running {
                        running_ids.push(session.id);
                    }
                }
            }
        }
        
        Ok(running_ids)
    }
}
