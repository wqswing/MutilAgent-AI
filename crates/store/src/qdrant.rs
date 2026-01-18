//! Qdrant vector database implementation.
//!
//! This module provides a production-ready vector store using Qdrant,
//! a high-performance, scalable vector database built in Rust.

use async_trait::async_trait;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, PointStruct, SearchPointsBuilder,
    UpsertPointsBuilder, VectorParamsBuilder, PointId, DeletePointsBuilder,
    PointsIdsList, Value as QdrantValue,
    vectors_config::Config as VectorsConfigEnum, VectorsConfig,
};
use qdrant_client::Qdrant;
use std::collections::HashMap;

use mutil_agent_core::{
    traits::{MemoryEntry, MemoryStore},
    Error, Result,
};

/// Qdrant-backed vector store for production RAG workloads.
pub struct QdrantMemoryStore {
    client: Qdrant,
    collection_name: String,
    vector_size: u64,
}

impl QdrantMemoryStore {
    /// Create a new Qdrant memory store.
    ///
    /// # Arguments
    /// * `url` - Qdrant server URL (e.g., "http://localhost:6334")
    /// * `collection_name` - Name of the collection to use
    /// * `vector_size` - Dimension of the embedding vectors (e.g., 1536 for OpenAI)
    pub async fn new(url: &str, collection_name: &str, vector_size: u64) -> Result<Self> {
        let client = Qdrant::from_url(url)
            .build()
            .map_err(|e| Error::storage(format!("Failed to connect to Qdrant: {}", e)))?;

        let store = Self {
            client,
            collection_name: collection_name.to_string(),
            vector_size,
        };

        // Ensure collection exists
        store.ensure_collection().await?;

        Ok(store)
    }

    /// Ensure the collection exists, creating it if necessary.
    async fn ensure_collection(&self) -> Result<()> {
        let collections = self.client
            .list_collections()
            .await
            .map_err(|e| Error::storage(format!("Failed to list collections: {}", e)))?;

        let exists = collections
            .collections
            .iter()
            .any(|c| c.name == self.collection_name);

        if !exists {
            tracing::info!(collection = %self.collection_name, "Creating Qdrant collection");
            
            let vectors_config = VectorsConfig {
                config: Some(VectorsConfigEnum::Params(
                    VectorParamsBuilder::new(self.vector_size, Distance::Cosine).build()
                )),
            };

            self.client
                .create_collection(
                    CreateCollectionBuilder::new(&self.collection_name)
                        .vectors_config(vectors_config)
                )
                .await
                .map_err(|e| Error::storage(format!("Failed to create collection: {}", e)))?;
        }

        Ok(())
    }

    /// Convert a HashMap<String, String> to Qdrant payload format.
    fn to_qdrant_payload(metadata: &HashMap<String, String>, content: &str) -> HashMap<String, QdrantValue> {
        let mut payload = HashMap::new();
        
        // Add content as a field
        payload.insert(
            "content".to_string(),
            QdrantValue {
                kind: Some(qdrant_client::qdrant::value::Kind::StringValue(content.to_string())),
            },
        );

        // Add all metadata fields
        for (key, value) in metadata {
            payload.insert(
                key.clone(),
                QdrantValue {
                    kind: Some(qdrant_client::qdrant::value::Kind::StringValue(value.clone())),
                },
            );
        }

        payload
    }

    /// Extract content and metadata from Qdrant payload.
    fn from_qdrant_payload(payload: &HashMap<String, QdrantValue>) -> (String, HashMap<String, String>) {
        let mut metadata = HashMap::new();
        let mut content = String::new();

        for (key, value) in payload {
            if let Some(qdrant_client::qdrant::value::Kind::StringValue(s)) = &value.kind {
                if key == "content" {
                    content = s.clone();
                } else {
                    metadata.insert(key.clone(), s.clone());
                }
            }
        }

        (content, metadata)
    }
}

#[async_trait]
impl MemoryStore for QdrantMemoryStore {
    async fn add(&self, entry: MemoryEntry) -> Result<()> {
        let point = PointStruct::new(
            entry.id.clone(),
            entry.embedding.clone(),
            Self::to_qdrant_payload(&entry.metadata, &entry.content),
        );

        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection_name, vec![point]))
            .await
            .map_err(|e| Error::storage(format!("Failed to upsert point: {}", e)))?;

        tracing::debug!(id = %entry.id, "Added entry to Qdrant");
        Ok(())
    }

    async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<MemoryEntry>> {
        let search_result = self.client
            .search_points(
                SearchPointsBuilder::new(&self.collection_name, query_embedding.to_vec(), limit as u64)
                    .with_payload(true)
                    .with_vectors(true)
            )
            .await
            .map_err(|e| Error::storage(format!("Failed to search: {}", e)))?;

        let entries = search_result
            .result
            .into_iter()
            .filter_map(|point| {
                let id = match point.id? {
                    PointId { point_id_options: Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(uuid)) } => uuid,
                    PointId { point_id_options: Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(num)) } => num.to_string(),
                    _ => return None,
                };

                // Extract embedding from VectorsOutput
                let embedding = point.vectors.and_then(|vo| {
                    use qdrant_client::qdrant::vectors_output::VectorsOptions;
                    use qdrant_client::qdrant::vector_output::Vector;
                    match vo.vectors_options? {
                        VectorsOptions::Vector(v) => {
                            // The VectorOutput has a .vector field which is Option<Vector enum>
                            match v.vector? {
                                Vector::Dense(dense) => Some(dense.data),
                                _ => None,
                            }
                        },
                        _ => None,
                    }
                })?;

                let (content, metadata) = Self::from_qdrant_payload(&point.payload);

                Some(MemoryEntry {
                    id,
                    content,
                    embedding,
                    metadata,
                })
            })
            .collect();

        Ok(entries)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        self.client
            .delete_points(
                DeletePointsBuilder::new(&self.collection_name)
                    .points(PointsIdsList {
                        ids: vec![PointId {
                            point_id_options: Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(id.to_string())),
                        }],
                    })
            )
            .await
            .map_err(|e| Error::storage(format!("Failed to delete point: {}", e)))?;

        tracing::debug!(id = %id, "Deleted entry from Qdrant");
        Ok(())
    }
}

/// Configuration for Qdrant connection.
#[derive(Debug, Clone)]
pub struct QdrantConfig {
    /// Qdrant server URL.
    pub url: String,
    /// Collection name.
    pub collection_name: String,
    /// Vector dimension (e.g., 1536 for OpenAI embeddings).
    pub vector_size: u64,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6334".to_string(),
            collection_name: "mutil_agent_memory".to_string(),
            vector_size: 1536,
        }
    }
}
