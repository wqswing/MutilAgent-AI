#![deny(unused)]
//! L-M Model Gateway for Multiagent.
//!
//! This crate provides:
//! - Model selection and load balancing
//! - Provider health tracking and circuit breaker
//! - Fallback and retry logic
//! - Rig LLM client adapter

pub mod providers;
pub mod rig_client;
pub mod selector;
pub mod pricing;

pub use providers::{MockLlmClient, ProviderRegistry};
pub use rig_client::{RigConfig, RigLlmClient, RigProvider, create_default_client};
pub use selector::AdaptiveModelSelector;
pub use pricing::{ModelPricing, PricingRegistry, SessionCostTracker};
