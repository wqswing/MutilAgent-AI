//! Adaptive model selector.

use async_trait::async_trait;
use std::sync::Arc;

use multi_agent_core::{
    traits::{LlmClient, ModelSelector},
    types::ModelTier,
    Error, Result,
};

use crate::providers::ProviderRegistry;

/// Selection strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStrategy {
    /// Prefer cheapest model.
    CostOptimized,
    /// Prefer fastest model.
    PerformanceOptimized,
    /// Balance cost and performance.
    Balanced,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        Self::Balanced
    }
}

/// Mapping from tier to provider priorities.
#[derive(Debug, Clone)]
pub struct TierMapping {
    /// Fast tier models.
    pub fast: Vec<String>,
    /// Standard tier models.
    pub standard: Vec<String>,
    /// Premium tier models.
    pub premium: Vec<String>,
}

impl Default for TierMapping {
    fn default() -> Self {
        Self {
            fast: vec![
                "openai:gpt-4o-mini".to_string(),
                "anthropic:claude-3-haiku-20240307".to_string(),
            ],
            standard: vec![
                "openai:gpt-4o".to_string(),
                "anthropic:claude-3-5-sonnet-20241022".to_string(),
            ],
            premium: vec![
                "anthropic:claude-3-5-sonnet-20241022".to_string(),
                "openai:gpt-4o".to_string(),
            ],
        }
    }
}

/// Adaptive model selector with fallback support.
pub struct AdaptiveModelSelector {
    /// Provider registry.
    registry: Arc<ProviderRegistry>,
    /// Selection strategy.
    strategy: SelectionStrategy,
    /// Tier mapping.
    tier_mapping: TierMapping,
}

impl AdaptiveModelSelector {
    /// Create a new model selector.
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        Self {
            registry,
            strategy: SelectionStrategy::default(),
            tier_mapping: TierMapping::default(),
        }
    }

    /// Set the selection strategy.
    pub fn with_strategy(mut self, strategy: SelectionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set custom tier mapping.
    pub fn with_tier_mapping(mut self, mapping: TierMapping) -> Self {
        self.tier_mapping = mapping;
        self
    }

    /// Get models for a tier.
    fn get_tier_models(&self, tier: ModelTier) -> &[String] {
        match tier {
            ModelTier::Fast => &self.tier_mapping.fast,
            ModelTier::Standard => &self.tier_mapping.standard,
            ModelTier::Premium => &self.tier_mapping.premium,
        }
    }


}

#[async_trait]
impl ModelSelector for AdaptiveModelSelector {
    async fn select(&self, tier: ModelTier) -> Result<Box<dyn LlmClient>> {
        let tier_models = self.get_tier_models(tier);
        let healthy = self.registry.get_healthy();

        // Find first healthy model from tier priority list
        for model_key in tier_models {
            if healthy.contains(model_key) {
                if let Some(entry) = self.registry.get_raw(model_key) {
                     // Create a CircuitBreakerClient wrapper
                     let client = crate::providers::CircuitBreakerClient::new(
                         entry,
                         self.registry.clone(),
                         model_key.clone()
                     );
                     
                     tracing::info!(model = %model_key, "Selected model with CB");
                     return Ok(Box::new(client));
                }
            }
        }

        // Try fallback to any healthy model
        if let Some(fallback_key) = healthy.first() {
            if let Some(entry) = self.registry.get_raw(fallback_key) {
                 tracing::warn!(
                    tier = ?tier,
                    fallback = %fallback_key,
                    "No tier model available, using fallback"
                );
                
                let client = crate::providers::CircuitBreakerClient::new(
                     entry,
                     self.registry.clone(),
                     fallback_key.clone()
                 );
                 return Ok(Box::new(client));
            }
        }

        Err(Error::AllProvidersUnavailable)
    }

    async fn report_failure(&self, provider: &str, model: &str) -> Result<()> {
        let key = format!("{}:{}", provider, model);
        self.registry.record_failure(&key);
        tracing::warn!(provider = provider, model = model, "Reported model failure");
        Ok(())
    }

    async fn report_success(&self, provider: &str, model: &str) -> Result<()> {
        let key = format!("{}:{}", provider, model);
        self.registry.record_success(&key);
        tracing::debug!(provider = provider, model = model, "Reported model success");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockLlmClient;

    #[tokio::test]
    async fn test_selector_no_providers() {
        let registry = Arc::new(ProviderRegistry::new());
        let selector = AdaptiveModelSelector::new(registry);

        let result = selector.select(ModelTier::Fast).await;
        assert!(matches!(result, Err(Error::AllProvidersUnavailable)));
    }

    #[tokio::test]
    async fn test_report_failure() {
        let registry = Arc::new(ProviderRegistry::new());
        registry.register("openai", "gpt-4o", Arc::new(MockLlmClient::new("test")));

        let selector = AdaptiveModelSelector::new(registry);

        selector.report_failure("openai", "gpt-4o").await.unwrap();
        // Provider should now be in degraded state
    }
}
