//! S3 implementation of ArtifactStore.

use async_trait::async_trait;
use aws_sdk_s3::{primitives::ByteStream, Client};
use bytes::Bytes;

use crate::retention::{Erasable, Prunable};
use multi_agent_core::{traits::ArtifactStore, types::RefId, Error, Result};

/// S3 storage for artifacts.
pub struct S3ArtifactStore {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3ArtifactStore {
    /// Create a new S3 artifact store.
    pub async fn new(bucket: &str, prefix: &str, endpoint: Option<&str>) -> Self {
        let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

        if let Some(url) = endpoint {
            config_loader = config_loader.endpoint_url(url);
        }

        let config = config_loader.load().await;

        // For MinIO/compatible backends, we often need force_path_style(true)
        // But aws-sdk-s3 v1+ usually handles this via config.
        // We'll trust the default behavior or standard env vars for now.
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

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| Error::storage(format!("S3 upload error: {}", e)))?;

        Ok(id)
    }

    async fn save_with_id(&self, id: &RefId, data: Bytes) -> Result<()> {
        let key = self.key(id);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| Error::storage(format!("S3 upload error: {}", e)))?;
        Ok(())
    }

    async fn save_with_type(&self, data: Bytes, content_type: &str) -> Result<RefId> {
        let id = RefId::new();
        let key = self.key(&id);

        self.client
            .put_object()
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

        let result = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(output) => {
                let data = output
                    .body
                    .collect()
                    .await
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

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| Error::storage(format!("S3 delete error: {}", e)))?;

        Ok(())
    }

    async fn exists(&self, id: &RefId) -> Result<bool> {
        let key = self.key(id);

        match self
            .client
            .head_object()
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

    async fn metadata(
        &self,
        id: &RefId,
    ) -> Result<Option<multi_agent_core::traits::ArtifactMetadata>> {
        let key = self.key(id);

        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(output) => {
                use multi_agent_core::traits::{ArtifactMetadata, StorageTier};

                Ok(Some(ArtifactMetadata {
                    size: output.content_length.unwrap_or(0) as usize,
                    content_type: output
                        .content_type
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
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

    async fn health_check(&self) -> Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| {
                Error::storage(format!(
                    "S3 health check failed for bucket '{}': {}",
                    self.bucket, e
                ))
            })?;

        Ok(())
    }
}

#[async_trait]
impl Prunable for S3ArtifactStore {
    async fn prune(&self, max_age: std::time::Duration) -> Result<usize> {
        let mut continuation_token = None;
        let mut count = 0;
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let cutoff = now_secs - max_age.as_secs() as i64;

        loop {
            let output = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&self.prefix)
                .set_continuation_token(continuation_token)
                .send()
                .await
                .map_err(|e| Error::storage(format!("S3 list error: {}", e)))?;

            let mut keys_to_delete = Vec::new();

            for object in output.contents.unwrap_or_default() {
                if let Some(last_modified) = object.last_modified {
                    if last_modified.secs() < cutoff {
                        if let Some(key) = object.key {
                            // construct ObjectIdentifier for batch delete
                            keys_to_delete.push(
                                aws_sdk_s3::types::ObjectIdentifier::builder()
                                    .key(key)
                                    .build()
                                    .unwrap(),
                            );
                        }
                    }
                }
            }

            if !keys_to_delete.is_empty() {
                let len = keys_to_delete.len();
                let delete = aws_sdk_s3::types::Delete::builder()
                    .set_objects(Some(keys_to_delete))
                    .build()
                    .map_err(|e| {
                        Error::storage(format!("Failed to build delete request: {}", e))
                    })?;

                self.client
                    .delete_objects()
                    .bucket(&self.bucket)
                    .delete(delete)
                    .send()
                    .await
                    .map_err(|e| Error::storage(format!("S3 delete objects error: {}", e)))?;

                count += len;
            }

            if output.is_truncated.unwrap_or(false) {
                continuation_token = output.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(count)
    }
}

#[async_trait]
impl Erasable for S3ArtifactStore {
    async fn erase_user(&self, user_id: &str) -> Result<usize> {
        let mut continuation_token = None;
        let mut count = 0;
        // Assume namespacing: prefix/user_id/
        let prefix = if self.prefix.is_empty() {
            format!("{}/", user_id)
        } else {
            format!("{}/{}/", self.prefix, user_id)
        };

        loop {
            let output = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&prefix)
                .set_continuation_token(continuation_token)
                .send()
                .await
                .map_err(|e| Error::storage(format!("S3 list error: {}", e)))?;

            let mut keys_to_delete = Vec::new();

            for object in output.contents.unwrap_or_default() {
                if let Some(key) = object.key {
                    keys_to_delete.push(
                        aws_sdk_s3::types::ObjectIdentifier::builder()
                            .key(key)
                            .build()
                            .unwrap(),
                    );
                }
            }

            if !keys_to_delete.is_empty() {
                let len = keys_to_delete.len();
                let delete = aws_sdk_s3::types::Delete::builder()
                    .set_objects(Some(keys_to_delete))
                    .build()
                    .map_err(|e| {
                        Error::storage(format!("Failed to build delete request: {}", e))
                    })?;

                self.client
                    .delete_objects()
                    .bucket(&self.bucket)
                    .delete(delete)
                    .send()
                    .await
                    .map_err(|e| Error::storage(format!("S3 delete objects error: {}", e)))?;

                count += len;
            }

            if output.is_truncated.unwrap_or(false) {
                continuation_token = output.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(count)
    }
}
