//! S3 implementation of ArtifactStore.

use async_trait::async_trait;
use aws_sdk_s3::{Client, primitives::ByteStream};
use bytes::Bytes;

use multi_agent_core::{
    traits::ArtifactStore,
    types::RefId,
    Error, Result,
};

/// S3 storage for artifacts.
pub struct S3ArtifactStore {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3ArtifactStore {
    /// Create a new S3 artifact store.
    pub async fn new(bucket: &str, prefix: &str) -> Self {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;
        let client = Client::new(&config);
        
        Self {
            client,
            bucket: bucket.to_string(),
            prefix: prefix.to_string(),
        }
    }

    /// Create with custom client (for testing/custom config).
    pub fn new_with_client(client: Client, bucket: &str, prefix: &str) -> Self {
        Self {
            client,
            bucket: bucket.to_string(),
            prefix: prefix.to_string(),
        }
    }

    fn key(&self, id: &RefId) -> String {
        if self.prefix.is_empty() {
            id.to_string()
        } else {
            format!("{}/{}", self.prefix, id)
        }
    }
}

#[async_trait]
impl ArtifactStore for S3ArtifactStore {
    async fn save(&self, data: Bytes) -> Result<RefId> {
        let id = RefId::new();
        let key = self.key(&id);
        
        self.client.put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| Error::storage(format!("S3 upload error: {}", e)))?;
            
        Ok(id)
    }

    async fn save_with_type(&self, data: Bytes, content_type: &str) -> Result<RefId> {
        let id = RefId::new();
        let key = self.key(&id);
        
        self.client.put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| Error::storage(format!("S3 upload error: {}", e)))?;
            
        Ok(id)
    }

    async fn load(&self, id: &RefId) -> Result<Option<Bytes>> {
        let key = self.key(id);
        
        let result = self.client.get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;
            
        match result {
            Ok(output) => {
                let data = output.body.collect().await
                    .map_err(|e| Error::storage(format!("S3 body read error: {}", e)))?
                    .into_bytes();
                Ok(Some(data))
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("NoSuchKey") || msg.contains("NotFound") || msg.contains("404") {
                    Ok(None)
                } else {
                    Err(Error::storage(format!("S3 download error: {}", e)))
                }
            }
        }
    }

    async fn delete(&self, id: &RefId) -> Result<()> {
        let key = self.key(id);
        
        self.client.delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| Error::storage(format!("S3 delete error: {}", e)))?;
            
        Ok(())
    }

    async fn exists(&self, id: &RefId) -> Result<bool> {
        let key = self.key(id);
        
        match self.client.head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("NoSuchKey") || msg.contains("NotFound") || msg.contains("404") {
                    Ok(false)
                } else {
                    Err(Error::storage(format!("S3 head error: {}", e)))
                }
            }
        }
    }

    async fn metadata(&self, id: &RefId) -> Result<Option<multi_agent_core::traits::ArtifactMetadata>> {
        let key = self.key(id);
        
        match self.client.head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(output) => {
                use multi_agent_core::traits::{ArtifactMetadata, StorageTier};
                
                Ok(Some(ArtifactMetadata {
                    size: output.content_length.unwrap_or(0) as usize,
                    content_type: output.content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                    created_at: output.last_modified.map(|d| d.secs()).unwrap_or(0),
                    tier: StorageTier::Cold,
                }))
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("NoSuchKey") || msg.contains("NotFound") || msg.contains("404") {
                     Ok(None)
                } else {
                    Err(Error::storage(format!("S3 metadata error: {}", e)))
                }
            }
        }
    }
}
