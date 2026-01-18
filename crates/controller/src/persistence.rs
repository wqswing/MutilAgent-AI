//! Session persistence for crash recovery.

use dashmap::DashMap;

use multi_agent_core::{
    traits::SessionStore,
    types::Session,
    Result,
};

/// In-memory session store.
pub struct InMemorySessionStore {
    sessions: DashMap<String, Session>,
}

impl InMemorySessionStore {
    /// Create a new in-memory session store.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Get the number of stored sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Check if store is empty.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SessionStore for InMemorySessionStore {
    async fn save(&self, session: &Session) -> Result<()> {
        self.sessions.insert(session.id.clone(), session.clone());
        tracing::debug!(session_id = %session.id, "Session saved");
        Ok(())
    }

    async fn load(&self, session_id: &str) -> Result<Option<Session>> {
        Ok(self.sessions.get(session_id).map(|r| r.clone()))
    }

    async fn delete(&self, session_id: &str) -> Result<()> {
        self.sessions.remove(session_id);
        tracing::debug!(session_id = %session_id, "Session deleted");
        Ok(())
    }

    async fn list_running(&self) -> Result<Vec<String>> {
        use multi_agent_core::types::SessionStatus;
        
        Ok(self
            .sessions
            .iter()
            .filter(|r| r.status == SessionStatus::Running)
            .map(|r| r.key().clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multi_agent_core::types::{SessionStatus, TokenUsage};

    fn create_test_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            status: SessionStatus::Running,
            history: vec![],
            task_state: None,
            token_usage: TokenUsage::with_budget(10000),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let store = InMemorySessionStore::new();
        let session = create_test_session("test-1");

        store.save(&session).await.unwrap();
        
        let loaded = store.load("test-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, "test-1");
    }

    #[tokio::test]
    async fn test_list_running() {
        let store = InMemorySessionStore::new();
        
        let running = create_test_session("s1");
        store.save(&running).await.unwrap();
        
        let mut completed = create_test_session("s2");
        completed.status = SessionStatus::Completed;
        store.save(&completed).await.unwrap();

        let running_list = store.list_running().await.unwrap();
        assert_eq!(running_list.len(), 1);
        assert!(running_list.contains(&"s1".to_string()));
    }
}
