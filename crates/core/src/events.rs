use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Structured Event Envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// Unique event ID
    pub id: String,
    /// Trace ID for distributed tracing (correlated across services)
    pub trace_id: String,
    /// Session ID (if applicable)
    pub session_id: Option<String>,
    /// Workspace ID (tenant isolation)
    pub workspace_id: Option<String>,
    /// Actor who triggered the event (user_id, tool_name, or 'system')
    pub actor: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Event type category
    pub event_type: EventType,
    /// Event severity level
    pub severity: EventSeverity,
    /// Structured payload (event-specific data)
    pub payload: serde_json::Value,
}

impl EventEnvelope {
    pub fn new(event_type: EventType, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            trace_id: Uuid::new_v4().to_string(), // Default, should be overwritten by context
            session_id: None,
            workspace_id: None,
            actor: "system".to_string(),
            timestamp: Utc::now(),
            event_type,
            severity: EventSeverity::Info,
            payload,
        }
    }

    pub fn with_trace(mut self, trace_id: &str) -> Self {
        self.trace_id = trace_id.to_string();
        self
    }

    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    pub fn with_workspace(mut self, workspace_id: &str) -> Self {
        self.workspace_id = Some(workspace_id.to_string());
        self
    }

    pub fn with_actor(mut self, actor: &str) -> Self {
        self.actor = actor.to_string();
        self
    }

    pub fn with_severity(mut self, severity: EventSeverity) -> Self {
        self.severity = severity;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    /// Received a new request from the user
    RequestReceived,
    /// Research task created
    ResearchCreated,
    /// User intent has been analyzed/resolved
    IntentResolved,
    /// Controller proposes a tool call
    ToolCallProposed,
    /// Research plan proposed by LLM
    PlanProposed,
    /// Policy engine evaluation result
    PolicyEvaluated,
    /// Manual approval requested
    ApprovalRequested,
    /// Manual approval decided (Approved/Rejected)
    ApprovalDecided,
    /// Tool execution started
    ToolExecStarted,
    /// Tool execution finished
    ToolExecFinished,
    /// Egress (network) request initiated
    EgressRequest,
    /// Egress (network) result received
    EgressResult,
    /// Filesystem read operation
    FsRead,
    /// Filesystem write operation
    FsWrite,
    /// Budget updated or checked
    BudgetUpdated,
    /// Budget limit exceeded
    BudgetExceeded,
    /// Audit log entry appended
    AuditAppended,
    /// Research report summary generated
    ReportGenerated,
    /// Audit export bundle generated
    ExportGenerated,
    /// Data deletion initiated (GDPR/Retention)
    DataDeletionInitiated,
    /// Data deletion completed
    DataDeletionCompleted,
    /// System error or exception
    SystemError,
    /// Generic/Other event
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

// Helper structs for common payloads

#[derive(Serialize, Deserialize)]
pub struct PolicyEvaluationPayload {
    pub tool_name: String,
    pub risk_level: String,
    pub risk_score: u32,
    pub matched_rule: Option<String>,
    pub reason: String,
    pub policy_version: String,
}

#[derive(Serialize, Deserialize)]
pub struct ToolExecPayload {
    pub tool_name: String,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct FsPayload {
    pub path: String,
    pub operation: String, // "read", "write", "list", "delete"
    pub size_bytes: Option<u64>,
    pub success: bool,
    pub error: Option<String>,
}
