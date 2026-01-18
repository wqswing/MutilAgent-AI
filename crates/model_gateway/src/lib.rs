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
pub mod config;

pub use providers::{MockLlmClient, ProviderRegistry};
pub use rig_client::{RigConfig, RigLlmClient, RigProvider, create_default_client};
pub use selector::AdaptiveModelSelector;
pub use pricing::{ModelPricing, PricingRegistry, SessionCostTracker};

use config::ProviderConfig;

/// Create an LLM client from configuration.
pub fn create_client_from_config(config: &ProviderConfig) -> multi_agent_core::Result<RigLlmClient> {
    // Simple strategy: Use the first provider/model found in the config
    // In the future, we could have a "default" flag or selection logic.
    
    for provider in &config.providers {
        match provider.name.to_lowercase().as_str() {
            "openai" => {
                 if let Some(model) = provider.models.first() {
                     return Ok(RigLlmClient::new(RigConfig::openai(&model.id)));
                 }
            }
            "anthropic" => {
                 if let Some(model) = provider.models.first() {
                     return Ok(RigLlmClient::new(RigConfig::anthropic(&model.id)));
                 }
            }
            _ => continue,
        }
    }
    
    Err(multi_agent_core::Error::ModelProvider("No supported provider found in config".to_string()))
}
