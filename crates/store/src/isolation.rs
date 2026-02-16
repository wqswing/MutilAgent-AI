//! Storage isolation wrappers.

use async_trait::async_trait;
use bytes::Bytes;
use multi_agent_core::{
    error::Result,
    traits::{ArtifactMetadata, ArtifactStore, SessionStore},
    types::{RefId, Session},
};
use std::sync::Arc;

/// An ArtifactStore that enforces keyspace isolation by prefixing all IDs with a namespace.
pub struct NamespacedArtifactStore<S> {
    inner: Arc<S>,
    namespace: String,
}

impl<S> NamespacedArtifactStore<S> {
    pub fn new(inner: Arc<S>, namespace: String) -> Self {
        Self { inner, namespace }
    }

    fn namespace_id(&self, id: &RefId) -> RefId {
        RefId::from_string(format!("{}/{}", self.namespace, id))
    }

    // Reverse operation isn't typically needed for load since we just pass the full ID
    // But if we list, we might need it. ArtifactStore doesn't support listing yet.
}

#[async_trait]
impl<S: ArtifactStore> ArtifactStore for NamespacedArtifactStore<S> {
    async fn save(&self, data: Bytes) -> Result<RefId> {
        // Generate a new ID (UUID)
        let uuid = RefId::new();
        // Create namespaced ID
        let ns_id = self.namespace_id(&uuid);

        // Save using the specific ID
        self.inner.save_with_id(&ns_id, data).await?;

        // Return the namespaced ID so retrieval works
        Ok(ns_id)
    }

    async fn save_with_id(&self, id: &RefId, data: Bytes) -> Result<()> {
        let ns_id = self.namespace_id(id);
        self.inner.save_with_id(&ns_id, data).await
    }

    async fn save_with_type(&self, data: Bytes, _content_type: &str) -> Result<RefId> {
        // We can't easily use inner.save_with_type because it generates its own ID.
        // We have to fallback to our save_with_id strategy, but we loose content-type
        // IF save_with_id doesn't support it.
        // ArtifactStore::save_with_type is a convenience wrapper usually.
        // Let's implement manually if possible or accept we lose content type?
        // Wait, S3 save_with_id doesn't take content type.
        // We should add save_with_id_and_type to trait? getting complicated.
        // For now, simple implementation:

        // Default impl: calls save() which uses default content type.
        // If we want type support, we need to extend trait again.
        // For now, let's treat it as save().
        self.save(data).await
    }

    async fn load(&self, id: &RefId) -> Result<Option<Bytes>> {
        // The ID passed here should already be namespaced if it came from save().
        // If the user manually constructed a RefId("uuid"), they won't find it.
        // They must pass RefId("namespace/uuid").

        // HOWEVER, if we want to force isolation even if they try to access "other/uuid",
        // we should verify the prefix?
        // But RefId is opaque.

        // If the system is designed such that `NamespacedStore` is the ONLY access point,
        // then `id` passed to load() is what was returned by save().
        // If save() returned "namespace/uuid", then load() gets "namespace/uuid".
        // We just pass it through.

        // BUT, if the user explicitly tries `load("alien_namespace/uuid")`, we allow it?
        // Security-wise, if the `NamespacedStore` is trusted to BE the view, it should enforced.

        // Validating prefix:
        if !id.as_str().starts_with(&format!("{}/", self.namespace)) {
            // Access denied or Not Found?
            // If we treat it as "key doesn't exist in this namespace", return None.
            return Ok(None);
        }

        self.inner.load(id).await
    }

    async fn delete(&self, id: &RefId) -> Result<()> {
        if !id.as_str().starts_with(&format!("{}/", self.namespace)) {
            return Ok(()); // Or error
        }
        self.inner.delete(id).await
    }

    async fn exists(&self, id: &RefId) -> Result<bool> {
        if !id.as_str().starts_with(&format!("{}/", self.namespace)) {
            return Ok(false);
        }
        self.inner.exists(id).await
    }

    async fn metadata(&self, id: &RefId) -> Result<Option<ArtifactMetadata>> {
        if !id.as_str().starts_with(&format!("{}/", self.namespace)) {
            return Ok(None);
        }
        self.inner.metadata(id).await
    }
}

/// A SessionStore that enforces keyspace isolation.
pub struct NamespacedSessionStore<S> {
    inner: Arc<S>,
    namespace: String,
}

impl<S> NamespacedSessionStore<S> {
    pub fn new(inner: Arc<S>, namespace: String) -> Self {
        Self { inner, namespace }
    }

    fn namespaced_key(&self, id: &str) -> String {
        format!("{}/{}", self.namespace, id)
    }
}

#[async_trait]
impl<S: SessionStore> SessionStore for NamespacedSessionStore<S> {
    async fn save(&self, session: &Session) -> Result<()> {
        // We must modify the session ID in the stored version?
        // Or does the store ignore the internal ID and use the key?
        // InMemorySessionStore uses session.id as key.
        // RedisSessionStore likely uses session.id.

        // If we clone and modify ID, we break integrity if the ID is used internally (e.g. references).

        // Better: We wrap the inner store's key generation if possible.
        // But traits don't expose key generation.

        // If we look at `InMemorySessionStore::save`:
        // self.sessions.insert(session.id.clone(), session.clone());

        // If we want isolation, we simply CANNOT use the same ID space.
        // But `Session` struct has `id` field.

        // If we want strict isolation without modifying `Session` object:
        // We rely on the `NamespacedSessionStore` being the only gateway.
        // But `SessionStore::save` signature doesn't take a key. It takes `Session`.

        // This suggests `SessionStore` implementation determines the key from `session.id`.
        // So we MUST modify `session.id` to namespace it.

        let mut ns_session = session.clone();
        ns_session.id = self.namespaced_key(&session.id);

        self.inner.save(&ns_session).await
    }

    async fn load(&self, session_id: &str) -> Result<Option<Session>> {
        // session_id passed here is the "short" ID (from user perspective)?
        // Or the full ID?
        // If the user only knows the short ID, and we namespace it transparently?
        // "Transparency" is hard if the ID is embedded in the object.

        // Let's assume the "ID" the system uses IS the namespaced ID.
        // i.e. The `Session` object carries the full `user/123` ID.
        // In that case, we just check the prefix.

        if !session_id.starts_with(&format!("{}/", self.namespace)) {
            return Ok(None);
        }

        self.inner.load(session_id).await
    }

    async fn delete(&self, session_id: &str) -> Result<()> {
        if !session_id.starts_with(&format!("{}/", self.namespace)) {
            return Ok(());
        }
        self.inner.delete(session_id).await
    }

    async fn list_running(&self) -> Result<Vec<String>> {
        let all = self.inner.list_running().await?;
        let prefix = format!("{}/", self.namespace);

        Ok(all
            .into_iter()
            .filter(|id| id.starts_with(&prefix))
            .collect())
    }

    async fn list_sessions(
        &self,
        status: Option<multi_agent_core::types::SessionStatus>,
        user_id: Option<&str>,
    ) -> Result<Vec<Session>> {
        let all = self.inner.list_sessions(status, user_id).await?;
        let prefix = format!("{}/", self.namespace);

        Ok(all
            .into_iter()
            .filter(|s| s.id.starts_with(&prefix))
            .collect())
    }
}
