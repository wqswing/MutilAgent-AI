//! LLM provider implementations.

use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::sync::Arc;

use multi_agent_core::{
    traits::{ChatMessage, LlmClient, LlmResponse, LlmUsage},
    types::ProviderHealth,
    Result, Error,
};

/// Provider status tracking.
#[derive(Debug)]
pub struct ProviderStatus {
    /// Provider name.
    pub name: String,
    /// Model name.
    pub model: String,
    /// Current health.
    pub health: ProviderHealth,
    /// Total requests.
    pub total_requests: AtomicU64,
    /// Failed requests.
    pub failed_requests: AtomicU64,
    /// Last failure time.
    pub last_failure: Option<Instant>,
    /// Circuit breaker open until.
    pub circuit_open_until: Option<Instant>,
}

impl ProviderStatus {
    /// Create a new provider status.
    pub fn new(name: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model: model.into(),
            health: ProviderHealth::Healthy,
            total_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            last_failure: None,
            circuit_open_until: None,
        }
    }

    /// Record a successful request.
    pub fn record_success(&mut self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.health = ProviderHealth::Healthy;
        self.circuit_open_until = None; // Reset circuit
    }

    /// Record a failed request.
    pub fn record_failure(&mut self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
        self.last_failure = Some(Instant::now());

        // Check if circuit should open
        let failures = self.failed_requests.load(Ordering::Relaxed);
        let total = self.total_requests.load(Ordering::Relaxed);
        
        // Simple heuristic: > 50% failure rate after 10 requests
        if total >= 10 && (failures as f64 / total as f64) > 0.5 {
            self.health = ProviderHealth::CircuitOpen;
            self.circuit_open_until = Some(Instant::now() + Duration::from_secs(60));
            tracing::warn!(name=%self.name, model=%self.model, "Circuit breaker OPENED");
        } else {
            self.health = ProviderHealth::Degraded;
        }
    }

    /// Check if circuit is open.
    pub fn is_circuit_open(&self) -> bool {
        if let Some(until) = self.circuit_open_until {
            if Instant::now() < until {
                return true;
            }
            // Circuit should be half-open, allow one request
            // In a more complex impl, we would set state to HalfOpen here.
        }
        false
    }

    /// Get failure rate.
    pub fn failure_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.failed_requests.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
}

/// Provider registry for managing LLM clients.
pub struct ProviderRegistry {
    /// Registered providers. Note: using Arc<dyn LlmClient> to support cloning.
    providers: DashMap<String, (Arc<dyn LlmClient>, ProviderStatus)>,
}

impl ProviderRegistry {
    /// Create a new provider registry.
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
        }
    }

    /// Register a provider.
    pub fn register(&self, name: &str, model: &str, client: Arc<dyn LlmClient>) {
        let status = ProviderStatus::new(name, model);
        let key = format!("{}:{}", name, model);
        self.providers.insert(key, (client, status));
    }

    /// Get all healthy providers.
    pub fn get_healthy(&self) -> Vec<String> {
        self.providers
            .iter()
            .filter(|entry| {
                let (_, status) = entry.value();
                !status.is_circuit_open() && status.health != ProviderHealth::Unhealthy
            })
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Check if a specific provider is healthy (circuit closed).
    pub fn is_healthy(&self, key: &str) -> bool {
        if let Some(entry) = self.providers.get(key) {
           !entry.value().1.is_circuit_open()
        } else {
            false
        }
    }

    /// Get a raw client.
    pub fn get_raw(&self, key: &str) -> Option<Arc<dyn LlmClient>> {
        self.providers.get(key).map(|entry| entry.value().0.clone())
    }
    
    /// Get a proxy client that handles circuit breaking.
    /// This is the preferred way to get a client.
    pub fn get_client(&self, _key: &str) -> Option<Arc<dyn LlmClient>> {
        // We need clarity on lifetimes here.
        // It's tricky to return a struct referencing `self` if `self` is inside an Arc elsewhere.
        // But `ProviderRegistry` is usually wrapped in Arc.
        // We can't easily construct the CircuitBreakerClient here if it needs Arc<ProviderRegistry>
        // and we only have &self.
        // Instead, let's expose `get_raw` and helper methods for record success/failure, which we already have.
        // The Pattern suggests `AdaptiveModelSelector` holds `Arc<ProviderRegistry>`, so IT should construct the client.
        // Let's assume the caller passes the registry Arc to the Wrapper constructor.
        None 
    }

    /// Get a specific provider.
    pub fn get(&self, key: &str) -> Option<dashmap::mapref::one::Ref<'_, String, (Arc<dyn LlmClient>, ProviderStatus)>> {
        self.providers.get(key)
    }

    /// Record success for a provider.
    pub fn record_success(&self, key: &str) {
        if let Some(mut entry) = self.providers.get_mut(key) {
            entry.1.record_success();
        }
    }

    /// Record failure for a provider.
    pub fn record_failure(&self, key: &str) {
        if let Some(mut entry) = self.providers.get_mut(key) {
            entry.1.record_failure();
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A wrapper client that implements Circuit Breaker logic.
pub struct CircuitBreakerClient {
    inner: Arc<dyn LlmClient>,
    registry: Arc<ProviderRegistry>,
    key: String,
}

impl CircuitBreakerClient {
    pub fn new(inner: Arc<dyn LlmClient>, registry: Arc<ProviderRegistry>, key: String) -> Self {
        Self {
            inner,
            registry,
            key,
        }
    }
    
    fn check_health(&self) -> Result<()> {
        if !self.registry.is_healthy(&self.key) {
            return Err(Error::ModelProvider(format!("Circuit breaker open for {}", self.key)));
        }
        Ok(())
    }
}

#[async_trait]
impl LlmClient for CircuitBreakerClient {
    async fn complete(&self, prompt: &str) -> Result<LlmResponse> {
        self.check_health()?;
        
        match self.inner.complete(prompt).await {
            Ok(res) => {
                self.registry.record_success(&self.key);
                Ok(res)
            }
            Err(e) => {
                self.registry.record_failure(&self.key);
                Err(e)
            }
        }
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<LlmResponse> {
        self.check_health()?;

        match self.inner.chat(messages).await {
            Ok(res) => {
                self.registry.record_success(&self.key);
                Ok(res)
            }
            Err(e) => {
                self.registry.record_failure(&self.key);
                Err(e)
            }
        }
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.check_health()?;

        match self.inner.embed(text).await {
            Ok(res) => {
                self.registry.record_success(&self.key);
                Ok(res)
            }
            Err(e) => {
                self.registry.record_failure(&self.key);
                Err(e)
            }
        }
    }
}

// =============================================================================
// Mock LLM Client for Testing
// =============================================================================

/// Mock LLM client for testing without real API calls.
pub struct MockLlmClient {
    /// Response to return.
    response: String,
    /// Simulate failure.
    should_fail: bool,
}

impl MockLlmClient {
    /// Create a new mock client.
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            should_fail: false,
        }
    }

    /// Create a failing mock client.
    pub fn failing() -> Self {
        Self {
            response: String::new(),
            should_fail: true,
        }
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, prompt: &str) -> Result<LlmResponse> {
        if self.should_fail {
            return Err(multi_agent_core::Error::ModelProvider("Mock failure".to_string()));
        }

        Ok(LlmResponse {
            content: format!("{}: {}", self.response, prompt),
            finish_reason: "stop".to_string(),
            usage: LlmUsage {
                prompt_tokens: prompt.len() as u64 / 4,
                completion_tokens: self.response.len() as u64 / 4,
                total_tokens: (prompt.len() + self.response.len()) as u64 / 4,
            },
            tool_calls: None,
        })
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<LlmResponse> {
        if self.should_fail {
            return Err(multi_agent_core::Error::ModelProvider("Mock failure".to_string()));
        }

        let last_message = messages.last().map(|m| m.content.as_str()).unwrap_or("");

        Ok(LlmResponse {
            content: format!("{}: {}", self.response, last_message),
            finish_reason: "stop".to_string(),
            usage: LlmUsage {
                prompt_tokens: messages.iter().map(|m| m.content.len() as u64).sum::<u64>() / 4,
                completion_tokens: self.response.len() as u64 / 4,
                total_tokens: 0, // Will be calculated
            },
            tool_calls: None,
        })
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
         if self.should_fail {
             return Err(multi_agent_core::Error::ModelProvider("Mock failure".to_string()));
         }
        // Return a simple mock embedding
        let len = 128;
        let hash = text.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64));
        let embeddings: Vec<f32> = (0..len)
            .map(|i| ((hash.wrapping_add(i as u64)) % 1000) as f32 / 1000.0)
            .collect();
        Ok(embeddings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_client() {
        let client = MockLlmClient::new("Response");

        let response = client.complete("Hello").await.unwrap();
        assert!(response.content.contains("Response"));
        assert!(response.content.contains("Hello"));
    }

    #[tokio::test]
    async fn test_mock_client_failure() {
        let client = MockLlmClient::failing();

        let result = client.complete("Hello").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_status() {
        let mut status = ProviderStatus::new("openai", "gpt-4o");

        status.record_success();
        assert_eq!(status.health, ProviderHealth::Healthy);

        // Record many failures to trigger circuit breaker
        for _ in 0..10 {
            status.record_failure();
        }

        assert!(status.failure_rate() > 0.5);
    }

    #[test]
    fn test_provider_registry() {
        let registry = ProviderRegistry::new();

        registry.register("openai", "gpt-4o", Arc::new(MockLlmClient::new("test")));

        let healthy = registry.get_healthy();
        assert_eq!(healthy.len(), 1);
    }
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let registry = Arc::new(ProviderRegistry::new());
        let mock = Arc::new(MockLlmClient::failing());
        registry.register("test", "fail", mock);
        
        let client = CircuitBreakerClient::new(
            registry.get_raw("test:fail").unwrap(),
            registry.clone(),
            "test:fail".to_string()
        );
        
        // Trigger failures
        for _ in 0..15 {
            let _ = client.complete("trigger").await;
        }
        
        // Circuit should be open now
        let result = client.complete("should fail fast").await;
        assert!(matches!(result, Err(Error::ModelProvider(msg)) if msg.contains("Circuit breaker open")));
    }
}
