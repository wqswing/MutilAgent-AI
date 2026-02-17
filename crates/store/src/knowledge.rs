//! In-memory Knowledge Store implementation.
//!
//! Uses cosine similarity for semantic search. Suitable for development
//! and small-scale deployments. For production, swap with a SQLite-vec
//! or Qdrant-backed implementation.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use multi_agent_core::{
    traits::{KnowledgeEntry, KnowledgeStore, Erasable},
    Result,
};

/// In-memory knowledge store with cosine similarity search.
pub struct InMemoryKnowledgeStore {
    entries: Arc<RwLock<Vec<KnowledgeEntry>>>,
}

impl InMemoryKnowledgeStore {
    /// Create a new empty knowledge store.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for InMemoryKnowledgeStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[async_trait]
impl KnowledgeStore for InMemoryKnowledgeStore {
    async fn store(&self, entry: KnowledgeEntry) -> Result<String> {
        let id = entry.id.clone();
        let mut entries = self.entries.write().await;
        // Upsert: replace if same ID exists
        entries.retain(|e| e.id != id);
        entries.push(entry);
        tracing::debug!(id = %id, total = entries.len(), "Knowledge entry stored");
        Ok(id)
    }

    async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<KnowledgeEntry>> {
        let entries = self.entries.read().await;

        let mut scored: Vec<(f32, &KnowledgeEntry)> = entries
            .iter()
            .map(|e| (cosine_similarity(query_embedding, &e.embedding), e))
            .collect();

        // Sort by similarity descending
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored
            .into_iter()
            .take(limit)
            .filter(|(score, _)| *score > 0.0) // Filter out zero-similarity results
            .map(|(_, entry)| entry.clone())
            .collect())
    }

    async fn search_by_tags(&self, tags: &[String], limit: usize) -> Result<Vec<KnowledgeEntry>> {
        let entries = self.entries.read().await;

        let results: Vec<KnowledgeEntry> = entries
            .iter()
            .filter(|e| tags.iter().any(|tag| e.tags.contains(tag)))
            .take(limit)
            .cloned()
            .collect();

        Ok(results)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.retain(|e| e.id != id);
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.entries.read().await.len())
    }
}

#[async_trait]
impl Erasable for InMemoryKnowledgeStore {
    async fn erase_user(&self, user_id: &str) -> Result<usize> {
        let mut entries = self.entries.write().await;
        let initial_len = entries.len();
        entries.retain(|e| e.user_id != user_id);
        Ok(initial_len - entries.len())
    }
}

use rusqlite::{params, Connection};

/// SQLite-backed knowledge store for persistent research summaries.
pub struct SqliteKnowledgeStore {
    conn: Arc<tokio::sync::Mutex<Connection>>,
}

impl SqliteKnowledgeStore {
    /// Create a new SQLite knowledge store at the given path.
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| multi_agent_core::error::Error::Internal(format!("DB error: {}", e)))?;

        // Initialize schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge (
                id TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                source_task TEXT NOT NULL,
                user_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                embedding TEXT NOT NULL, -- JSON array
                tags TEXT NOT NULL,      -- JSON array
                created_at INTEGER NOT NULL
            )",
            [],
        ).map_err(|e| multi_agent_core::error::Error::Internal(format!("Schema error: {}", e)))?;

        // Index for tag and user search
        conn.execute("CREATE INDEX IF NOT EXISTS idx_knowledge_session ON knowledge (session_id)", [])
            .map_err(|e| multi_agent_core::error::Error::Internal(format!("Index error: {}", e)))?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_knowledge_user ON knowledge (user_id)", [])
            .map_err(|e| multi_agent_core::error::Error::Internal(format!("Index error: {}", e)))?;

        Ok(Self {
            conn: Arc::new(tokio::sync::Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl KnowledgeStore for SqliteKnowledgeStore {
    async fn store(&self, entry: KnowledgeEntry) -> Result<String> {
        let conn = self.conn.clone();
        let id = entry.id.clone();
        
        // Convert vectors to JSON strings for storage
        let embedding_json = serde_json::to_string(&entry.embedding)
            .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?;
        let tags_json = serde_json::to_string(&entry.tags)
            .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?;

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO knowledge (id, summary, source_task, user_id, session_id, embedding, tags, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    entry.id,
                    entry.summary,
                    entry.source_task,
                    entry.user_id,
                    entry.session_id,
                    embedding_json,
                    tags_json,
                    entry.created_at
                ],
            ).map_err(|e| multi_agent_core::error::Error::Internal(format!("Insert error: {}", e)))?;
            Ok(id)
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }

    async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<KnowledgeEntry>> {
        let conn = self.conn.clone();
        let query_vec = query_embedding.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, summary, source_task, user_id, session_id, embedding, tags, created_at FROM knowledge"
            ).map_err(|e| multi_agent_core::error::Error::Internal(format!("Prepare error: {}", e)))?;

            let entries = stmt.query_map([], |row| {
                let embedding_str: String = row.get(5)?;
                let tags_str: String = row.get(6)?;
                
                Ok(KnowledgeEntry {
                    id: row.get(0)?,
                    summary: row.get(1)?,
                    source_task: row.get(2)?,
                    user_id: row.get(3)?,
                    session_id: row.get(4)?,
                    embedding: serde_json::from_str(&embedding_str).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    created_at: row.get(7)?,
                })
            }).map_err(|e| multi_agent_core::error::Error::Internal(format!("Query error: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| multi_agent_core::error::Error::Internal(format!("Result error: {}", e)))?;

            // Compute similarity in-memory (SQLite-vec is preferred but we use manual approach for now)
            let mut scored: Vec<(f32, KnowledgeEntry)> = entries
                .into_iter()
                .map(|e| (cosine_similarity(&query_vec, &e.embedding), e))
                .collect();

            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            Ok(scored
                .into_iter()
                .take(limit)
                .filter(|(score, _)| *score > 0.0)
                .map(|(_, entry)| entry)
                .collect())
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }

    async fn search_by_tags(&self, tags: &[String], limit: usize) -> Result<Vec<KnowledgeEntry>> {
        let conn = self.conn.clone();
        let search_tags = tags.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, summary, source_task, user_id, session_id, embedding, tags, created_at FROM knowledge"
            ).map_err(|e| multi_agent_core::error::Error::Internal(format!("Prepare error: {}", e)))?;

            let entries = stmt.query_map([], |row| {
                let embedding_str: String = row.get(5)?;
                let tags_str: String = row.get(6)?;
                
                Ok(KnowledgeEntry {
                    id: row.get(0)?,
                    summary: row.get(1)?,
                    source_task: row.get(2)?,
                    user_id: row.get(3)?,
                    session_id: row.get(4)?,
                    embedding: serde_json::from_str(&embedding_str).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    created_at: row.get(7)?,
                })
            }).map_err(|e| multi_agent_core::error::Error::Internal(format!("Query error: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| multi_agent_core::error::Error::Internal(format!("Result error: {}", e)))?;

            Ok(entries
                .into_iter()
                .filter(|e| search_tags.iter().any(|tag| e.tags.contains(tag)))
                .take(limit)
                .collect())
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let target_id = id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute("DELETE FROM knowledge WHERE id = ?1", params![target_id])
                .map_err(|e| multi_agent_core::error::Error::Internal(format!("Delete error: {}", e)))?;
            Ok(())
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }

    async fn count(&self) -> Result<usize> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let count: usize = conn.query_row("SELECT COUNT(*) FROM knowledge", [], |row| row.get(0))
                .map_err(|e| multi_agent_core::error::Error::Internal(format!("Count error: {}", e)))?;
            Ok(count)
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }
}

#[async_trait]
impl Erasable for SqliteKnowledgeStore {
    async fn erase_user(&self, user_id: &str) -> Result<usize> {
        let conn = self.conn.clone();
        let uid = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let count = conn.execute("DELETE FROM knowledge WHERE user_id = ?", params![uid])
                .map_err(|e| multi_agent_core::error::Error::Internal(format!("Delete error: {}", e)))?;
            Ok(count)
        })
        .await
        .map_err(|e| multi_agent_core::error::Error::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, summary: &str, embedding: Vec<f32>, tags: Vec<&str>) -> KnowledgeEntry {
        KnowledgeEntry {
            id: id.to_string(),
            summary: summary.to_string(),
            source_task: "test task".to_string(),
            user_id: "user-1".to_string(),
            session_id: "session-1".to_string(),
            embedding,
            tags: tags.into_iter().map(String::from).collect(),
            created_at: 1000,
        }
    }

    #[tokio::test]
    async fn test_store_and_count() {
        let store = InMemoryKnowledgeStore::new();
        assert_eq!(store.count().await.unwrap(), 0);

        store
            .store(make_entry(
                "k1",
                "Rust is fast",
                vec![1.0, 0.0, 0.0],
                vec!["lang"],
            ))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        store
            .store(make_entry(
                "k2",
                "Python is flexible",
                vec![0.0, 1.0, 0.0],
                vec!["lang"],
            ))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_upsert() {
        let store = InMemoryKnowledgeStore::new();

        store
            .store(make_entry("k1", "v1", vec![1.0], vec![]))
            .await
            .unwrap();
        store
            .store(make_entry("k1", "v2", vec![1.0], vec![]))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        // The updated entry should have v2
        let results = store.search(&[1.0], 10).await.unwrap();
        assert_eq!(results[0].summary, "v2");
    }

    #[tokio::test]
    async fn test_semantic_search() {
        let store = InMemoryKnowledgeStore::new();

        // Create entries with orthogonal embeddings
        store
            .store(make_entry("k1", "Rust", vec![1.0, 0.0, 0.0], vec![]))
            .await
            .unwrap();
        store
            .store(make_entry("k2", "Python", vec![0.0, 1.0, 0.0], vec![]))
            .await
            .unwrap();
        store
            .store(make_entry("k3", "Go", vec![0.0, 0.0, 1.0], vec![]))
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 1); // Only k1 should match (others have 0 similarity)
        assert_eq!(results[0].id, "k1");

        // A mixed query should find the closest
        let results = store.search(&[0.7, 0.7, 0.0], 3).await.unwrap();
        assert_eq!(results.len(), 2);
        // Both k1 and k2 should match, k3 has 0 similarity
    }

    #[tokio::test]
    async fn test_tag_search() {
        let store = InMemoryKnowledgeStore::new();

        store
            .store(make_entry("k1", "Rust", vec![1.0], vec!["systems", "fast"]))
            .await
            .unwrap();
        store
            .store(make_entry(
                "k2",
                "Python",
                vec![0.0],
                vec!["scripting", "fast"],
            ))
            .await
            .unwrap();
        store
            .store(make_entry("k3", "SQL", vec![0.0], vec!["database"]))
            .await
            .unwrap();

        let results = store
            .search_by_tags(&["fast".to_string()], 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        let results = store
            .search_by_tags(&["database".to_string()], 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "k3");
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryKnowledgeStore::new();

        store
            .store(make_entry("k1", "v1", vec![1.0], vec![]))
            .await
            .unwrap();
        store
            .store(make_entry("k2", "v2", vec![1.0], vec![]))
            .await
            .unwrap();

        store.delete("k1").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        // Deleting non-existent ID should not error
        store.delete("nonexistent").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_knowledge_store() {
        use tempfile::NamedTempFile;
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        let store = SqliteKnowledgeStore::new(path).unwrap();

        assert_eq!(store.count().await.unwrap(), 0);

        store
            .store(make_entry("k1", "Sqlite is persistent", vec![1.0], vec!["db"]))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        let results = store.search(&[1.0], 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "k1");
        assert_eq!(results[0].summary, "Sqlite is persistent");

        store.delete("k1").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
    }
}
