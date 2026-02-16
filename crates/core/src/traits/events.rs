use async_trait::async_trait;
use crate::events::EventEnvelope;

/// Trait for emitting structured events.
#[async_trait]
pub trait EventEmitter: Send + Sync {
    /// Emit an event.
    async fn emit(&self, event: EventEnvelope);
}

/// No-op implementation for testing/default.
pub struct NoOpEventEmitter;

#[async_trait]
impl EventEmitter for NoOpEventEmitter {
    async fn emit(&self, _event: EventEnvelope) {}
}
