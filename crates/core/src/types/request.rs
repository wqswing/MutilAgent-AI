use super::refs::RefId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Request Types
// =============================================================================

/// Content type for multi-modal input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum RequestContent {
    /// Plain text content.
    Text(String),

    /// Audio content (will be Whisper transcribed).
    Audio {
        /// Reference to audio file in L3.
        ref_id: RefId,
        /// Optional transcription if already processed.
        transcription: Option<String>,
    },

    /// Image content (will be Vision parsed).
    Image {
        /// Reference to image file in L3.
        ref_id: RefId,
        /// Optional description if already processed.
        description: Option<String>,
    },

    /// System event (webhook payload).
    SystemEvent {
        /// Event type identifier.
        event_type: String,
        /// Event payload.
        payload: serde_json::Value,
    },
}

/// Normalized input request after multi-modal processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedRequest {
    /// Unique trace ID for this request.
    pub trace_id: String,

    /// Normalized content (always text after processing).
    pub content: String,

    /// Original content type.
    pub original_content: RequestContent,

    /// References to artifacts in L3.
    pub refs: Vec<RefId>,

    /// Request metadata.
    pub metadata: RequestMetadata,
}

/// Metadata associated with a request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMetadata {
    /// User identifier.
    pub user_id: Option<String>,

    /// Workspace identifier for multi-tenancy isolation.
    pub workspace_id: Option<String>,

    /// Session identifier for stateful conversations.
    pub session_id: Option<String>,

    /// Trace identifier for distributed tracing.
    pub trace_id: Option<String>,

    /// Custom key-value metadata.
    pub custom: std::collections::HashMap<String, String>,
}

impl NormalizedRequest {
    /// Create a new NormalizedRequest with text content.
    pub fn text(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            trace_id: Uuid::new_v4().to_string(),
            content: content.clone(),
            original_content: RequestContent::Text(content),
            refs: Vec::new(),
            metadata: RequestMetadata::default(),
        }
    }

    /// Add a reference to the request.
    pub fn with_ref(mut self, ref_id: RefId) -> Self {
        self.refs.push(ref_id);
        self
    }

    /// Add metadata to the request.
    pub fn with_metadata(mut self, metadata: RequestMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}
