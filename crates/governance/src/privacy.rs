//! Privacy and GDPR compliance logic.

use multi_agent_core::events::{EventEnvelope, EventSeverity, EventType};
use multi_agent_core::traits::events::EventEmitter;
use multi_agent_core::traits::store::Erasable;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Report of a data deletion operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionReport {
    pub user_id: String,
    pub total_deleted: usize,
    pub errors: Vec<String>,
}

/// Controller for privacy operations.
/// Controller for privacy operations.
pub struct PrivacyController {
    stores: Vec<Arc<dyn Erasable>>,
    event_emitter: Arc<dyn EventEmitter>,
}

impl PrivacyController {
    /// Create a new privacy controller with the given stores.
    pub fn new(stores: Vec<Arc<dyn Erasable>>, event_emitter: Arc<dyn EventEmitter>) -> Self {
        Self {
            stores,
            event_emitter,
        }
    }

    /// Execute the "Right to be Forgotten" for a user.
    ///
    /// This will attempt to delete all data associated with the user from all registered stores.
    pub async fn forget_user(&self, user_id: &str) -> DeletionReport {
        // Emit initiation event
        let init_event = EventEnvelope::new(
            EventType::DataDeletionInitiated,
            serde_json::json!({ "user_id": user_id }),
        )
        .with_severity(EventSeverity::Warning)
        .with_actor("system");
        self.event_emitter.emit(init_event).await;

        let mut report = DeletionReport {
            user_id: user_id.to_string(),
            total_deleted: 0,
            errors: Vec::new(),
        };

        for store in &self.stores {
            match store.erase_user(user_id).await {
                Ok(count) => {
                    report.total_deleted += count;
                }
                Err(e) => {
                    report.errors.push(format!("Store error: {}", e));
                }
            }
        }

        // Emit completion event
        let complete_event = EventEnvelope::new(
            EventType::DataDeletionCompleted,
            serde_json::json!({
                "user_id": user_id,
                "total_deleted": report.total_deleted,
                "errors": report.errors
            }),
        )
        .with_severity(EventSeverity::Info)
        .with_actor("system");
        self.event_emitter.emit(complete_event).await;

        report
    }
}

// For now, I will create this file and then go back to update retention.rs with Erasable trait.
// And then implement Erasable for stores.
