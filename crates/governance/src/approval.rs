//! HITL (Human-in-the-Loop) approval gate implementations.
//!
//! Provides mechanisms for human review and approval of high-risk tool calls.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, Mutex};

use multi_agent_core::{
    traits::ApprovalGate,
    types::{ApprovalRequest, ApprovalResponse, ToolRiskLevel},
    Error, Result,
};

// =============================================================================
// Channel-Based Approval Gate
// =============================================================================

/// Approval gate that uses channels for async notification.
///
/// When a tool requires approval, a request is published to listeners
/// (e.g., a WebSocket handler) and the execution pauses until a response
/// arrives via the oneshot channel.
pub struct ChannelApprovalGate {
    /// Minimum risk level that triggers approval.
    threshold: ToolRiskLevel,
    /// Pending approval requests, keyed by request_id.
    pending: Arc<Mutex<HashMap<String, (oneshot::Sender<ApprovalResponse>, String)>>>,
    /// Broadcast channel for notifying listeners about new requests.
    request_tx: broadcast::Sender<ApprovalRequest>,
    /// Timeout for waiting for approval (default: 5 minutes).
    timeout: std::time::Duration,
}

impl ChannelApprovalGate {
    /// Create a new channel-based approval gate.
    pub fn new(threshold: ToolRiskLevel) -> Self {
        let (request_tx, _) = broadcast::channel(32);
        Self {
            threshold,
            pending: Arc::new(Mutex::new(HashMap::new())),
            request_tx,
            timeout: std::time::Duration::from_secs(300), // 5 minutes
        }
    }

    /// Set the approval timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Subscribe to approval request notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<ApprovalRequest> {
        self.request_tx.subscribe()
    }

    /// Submit a human's response to a pending approval request.
    ///
    /// Called by WebSocket/REST handlers when the human reviews a request.
    pub async fn submit_response(
        &self,
        request_id: &str,
        nonce: &str,
        response: ApprovalResponse,
    ) -> std::result::Result<(), String> {
        let mut pending = self.pending.lock().await;
        match pending.remove(request_id) {
            Some((sender, stored_nonce)) => {
                if stored_nonce != nonce {
                    return Err("Invalid nonce".to_string());
                }
                sender
                    .send(response)
                    .map_err(|_| "Request channel closed (agent may have timed out)".to_string())
            }
            None => Err(format!("No pending request with ID: {}", request_id)),
        }
    }

    /// Get the list of currently pending approval requests.
    pub async fn list_pending(&self) -> Vec<String> {
        self.pending.lock().await.keys().cloned().collect()
    }
}

#[async_trait]
impl ApprovalGate for ChannelApprovalGate {
    async fn request_approval(&self, req: &ApprovalRequest) -> Result<ApprovalResponse> {
        let (tx, rx) = oneshot::channel();

        // Register the pending request
        {
            let mut pending = self.pending.lock().await;
            pending.insert(req.request_id.clone(), (tx, req.nonce.clone()));
        }

        // Notify listeners (WebSocket, etc.)
        let _ = self.request_tx.send(req.clone());

        tracing::info!(
            request_id = %req.request_id,
            tool = %req.tool_name,
            risk = ?req.risk_level,
            "Waiting for human approval (timeout: {:?})",
            self.timeout
        );

        // Wait for response with timeout
        let timeout = req
            .timeout_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(self.timeout);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                // Channel dropped — clean up
                self.pending.lock().await.remove(&req.request_id);
                Err(Error::governance("Approval channel closed unexpectedly"))
            }
            Err(_) => {
                // Timeout — auto-deny
                self.pending.lock().await.remove(&req.request_id);
                tracing::warn!(
                    request_id = %req.request_id,
                    "Approval request timed out — auto-denied"
                );
                Ok(ApprovalResponse::Denied {
                    reason: "Approval timed out (auto-denied for safety)".to_string(),
                    reason_code: "TIMEOUT".to_string(),
                })
            }
        }
    }

    fn threshold(&self) -> ToolRiskLevel {
        self.threshold
    }
}

// =============================================================================
// Auto-Approve Gate (for development/testing)
// =============================================================================

/// Approval gate that auto-approves all requests.
///
/// Use only in development/testing environments.
pub struct AutoApproveGate;

#[async_trait]
impl ApprovalGate for AutoApproveGate {
    async fn request_approval(&self, req: &ApprovalRequest) -> Result<ApprovalResponse> {
        tracing::warn!(
            tool = %req.tool_name,
            risk = ?req.risk_level,
            "AUTO-APPROVED (development mode — do NOT use in production)"
        );
        Ok(ApprovalResponse::Approved {
            reason: Some("Auto-approved in development mode".to_string()),
            reason_code: "AUTO_APPROVED".to_string(),
        })
    }

    fn threshold(&self) -> ToolRiskLevel {
        ToolRiskLevel::Critical // Only trigger on Critical in dev mode
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auto_approve_gate() {
        let gate = AutoApproveGate;
        let req = ApprovalRequest {
            request_id: "test-1".into(),
            session_id: "session-1".into(),
            tool_name: "sandbox_shell".into(),
            args: serde_json::json!({"command": "rm -rf /"}),
            risk_level: ToolRiskLevel::Critical,
            context: "test".into(),
            timeout_secs: None,
            nonce: "test-nonce-1".into(),
            expires_at: 0,
        };

        let response = gate.request_approval(&req).await.unwrap();
        assert!(matches!(response, ApprovalResponse::Approved { .. }));
    }

    #[tokio::test]
    async fn test_channel_gate_submit_response() {
        let gate = ChannelApprovalGate::new(ToolRiskLevel::High)
            .with_timeout(std::time::Duration::from_secs(10));

        let req = ApprovalRequest {
            request_id: "test-2".into(),
            session_id: "session-1".into(),
            tool_name: "sandbox_shell".into(),
            args: serde_json::json!({"command": "ls"}),
            risk_level: ToolRiskLevel::High,
            context: "test".into(),
            timeout_secs: None,
            nonce: "test-nonce-2".into(),
            expires_at: 0,
        };

        // Spawn the approval request
        let gate_clone = Arc::new(gate);
        let gate_for_task = gate_clone.clone();
        let req_clone = req.clone();

        let handle = tokio::spawn(async move { gate_for_task.request_approval(&req_clone).await });

        // Give the request time to register
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Submit approval
        gate_clone
            .submit_response(
                "test-2",
                "test-nonce-2",
                ApprovalResponse::Approved {
                    reason: None,
                    reason_code: "USER_APPROVED".into(),
                },
            )
            .await
            .unwrap();

        let response = handle.await.unwrap().unwrap();
        assert!(matches!(response, ApprovalResponse::Approved { .. }));
    }

    #[tokio::test]
    async fn test_channel_gate_denial() {
        let gate = Arc::new(
            ChannelApprovalGate::new(ToolRiskLevel::High)
                .with_timeout(std::time::Duration::from_secs(10)),
        );

        let req = ApprovalRequest {
            request_id: "test-3".into(),
            session_id: "session-1".into(),
            tool_name: "sandbox_shell".into(),
            args: serde_json::json!({"command": "rm -rf /"}),
            risk_level: ToolRiskLevel::High,
            context: "test".into(),
            timeout_secs: None,
            nonce: "test-nonce-3".into(),
            expires_at: 0,
        };

        let gate_for_task = gate.clone();
        let req_clone = req.clone();

        let handle = tokio::spawn(async move { gate_for_task.request_approval(&req_clone).await });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        gate.submit_response(
            "test-3",
            "test-nonce-3",
            ApprovalResponse::Denied {
                reason: "too dangerous".into(),
                reason_code: "USER_DENIED".into(),
            },
        )
        .await
        .unwrap();

        let response = handle.await.unwrap().unwrap();
        match response {
            ApprovalResponse::Denied { reason, .. } => assert_eq!(reason, "too dangerous"),
            _ => panic!("Expected Denied"),
        }
    }

    #[tokio::test]
    async fn test_channel_gate_timeout() {
        let gate = ChannelApprovalGate::new(ToolRiskLevel::High)
            .with_timeout(std::time::Duration::from_millis(200));

        let req = ApprovalRequest {
            request_id: "test-4".into(),
            session_id: "session-1".into(),
            tool_name: "sandbox_shell".into(),
            args: serde_json::json!({"command": "ls"}),
            risk_level: ToolRiskLevel::High,
            context: "test".into(),
            timeout_secs: None,
            nonce: "test-nonce-4".into(),
            expires_at: 0,
        };

        // Don't submit any response — should timeout
        let response = gate.request_approval(&req).await.unwrap();
        match response {
            ApprovalResponse::Denied {
                reason,
                reason_code,
            } => {
                assert!(reason.contains("timed out"));
                assert_eq!(reason_code, "TIMEOUT");
            }
            _ => panic!("Expected Denied due to timeout"),
        }
    }
}
