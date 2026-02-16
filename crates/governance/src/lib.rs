#![deny(unused)]
//! L4 Governance for Multiagent.
//!
//! This crate provides:
//! - Budget control (token limits)
//! - Security proxy (request validation)
//! - Distributed tracing
//! - RBAC connector for enterprise IAM
//! - Audit logging
//! - Encrypted secrets management

pub mod budget;
pub mod security;
pub mod network;
pub mod tracing_layer;
pub mod metrics;
pub mod guardrails;
pub mod policy;
pub mod rbac;
pub mod audit;
pub mod secrets;
pub mod approval;

pub use budget::TokenBudgetController;
pub use security::DefaultSecurityProxy;
pub use tracing_layer::configure_tracing;
pub use metrics::{setup_metrics_recorder, track_request, track_tokens};
pub use guardrails::{Guardrail, GuardrailResult, ViolationType, PiiScanner, PromptInjectionDetector, CompositeGuardrail};
pub use rbac::{RbacConnector, UserRoles, NoOpRbacConnector, StaticTokenRbacConnector};
pub use audit::{AuditStore, AuditEntry, AuditOutcome, AuditFilter, InMemoryAuditStore, FileAuditStore};
pub use policy::{PolicyEngine, PolicyFile, PolicyDecision, PolicyRule, RuleMatch, RuleAction};
pub use secrets::{SecretsManager, EncryptedSecret, AesGcmSecretsManager};
pub use approval::{ChannelApprovalGate, AutoApproveGate};

