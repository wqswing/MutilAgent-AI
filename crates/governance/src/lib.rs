#![deny(unused)]
//! L4 Governance for Multiagent.
//!
//! This crate provides:
//! - Budget control (token limits)
//! - Security proxy (request validation)
//! - Distributed tracing

pub mod budget;
pub mod security;
pub mod tracing_layer;
pub mod metrics;
pub mod guardrails;

pub use budget::TokenBudgetController;
pub use security::DefaultSecurityProxy;
pub use tracing_layer::configure_tracing;
pub use metrics::{setup_metrics_recorder, track_request, track_tokens};
pub use guardrails::{Guardrail, GuardrailResult, ViolationType, PiiScanner, PromptInjectionDetector, CompositeGuardrail};
