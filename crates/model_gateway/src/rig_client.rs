//! Rig LLM client adapter.
//!
//! Wraps Rig's Agent for integration with our LlmClient trait.

use async_trait::async_trait;

use multi_agent_core::{
    traits::{ChatMessage, LlmClient, LlmResponse, LlmUsage},
    Error, Result,
};

// Import required Rig traits
use rig::client::{CompletionClient, EmbeddingsClient, ProviderClient};
use rig::completion::Prompt;

/// Provider type for Rig clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigProvider {
    OpenAI,
    Anthropic,
}

/// Configuration for Rig client.
#[derive(Debug, Clone)]
pub struct RigConfig {
    /// Provider to use.
    pub provider: RigProvider,
    /// Model name.
    pub model: String,
    /// System prompt.
    pub system_prompt: Option<String>,
    /// Temperature (0.0 - 1.0).
    pub temperature: Option<f32>,
    /// Max tokens.
    pub max_tokens: Option<u32>,
}

impl Default for RigConfig {
    fn default() -> Self {
        Self {
            provider: RigProvider::OpenAI,
            model: "gpt-4o-mini".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(4096),
        }
    }
}

impl RigConfig {
    /// Create config for OpenAI.
    pub fn openai(model: impl Into<String>) -> Self {
        Self {
            provider: RigProvider::OpenAI,
            model: model.into(),
            ..Default::default()
        }
    }

    /// Create config for Anthropic.
    pub fn anthropic(model: impl Into<String>) -> Self {
        Self {
            provider: RigProvider::Anthropic,
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set temperature.
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }
}

/// Rig-based LLM client.
///
/// This client wraps Rig's provider clients to implement our LlmClient trait,
/// providing a unified interface for LLM calls across the system.
pub struct RigLlmClient {
    config: RigConfig,
}

impl RigLlmClient {
    /// Create a new Rig client with the given configuration.
    pub fn new(config: RigConfig) -> Self {
        Self { config }
    }

    /// Create a client for OpenAI GPT-4o.
    pub fn gpt4o() -> Self {
        Self::new(RigConfig::openai("gpt-4o"))
    }

    /// Create a client for OpenAI GPT-4o-mini.
    pub fn gpt4o_mini() -> Self {
        Self::new(RigConfig::openai("gpt-4o-mini"))
    }

    /// Create a client for Claude Sonnet.
    pub fn claude_sonnet() -> Self {
        Self::new(RigConfig::anthropic("claude-3-5-sonnet-20241022"))
    }

    /// Create a client for Claude Haiku.
    pub fn claude_haiku() -> Self {
        Self::new(RigConfig::anthropic("claude-3-haiku-20240307"))
    }

    /// Build messages into a prompt string.
    fn build_prompt(&self, messages: &[ChatMessage]) -> String {
        let mut prompt = String::new();

        for msg in messages {
            match msg.role.as_str() {
                "system" => {
                    prompt.push_str(&format!("System: {}\n\n", msg.content));
                }
                "user" => {
                    prompt.push_str(&format!("User: {}\n\n", msg.content));
                }
                "assistant" => {
                    prompt.push_str(&format!("Assistant: {}\n\n", msg.content));
                }
                "tool" => {
                    prompt.push_str(&format!("Tool Result: {}\n\n", msg.content));
                }
                _ => {
                    prompt.push_str(&format!("{}: {}\n\n", msg.role, msg.content));
                }
            }
        }

        prompt
    }

    /// Call OpenAI via Rig.
    async fn call_openai(&self, prompt: &str) -> Result<LlmResponse> {
        use rig::providers::openai;

        // Check env var first to avoid panic
        if std::env::var("OPENAI_API_KEY").is_err() {
            return Err(Error::ModelProvider("OPENAI_API_KEY not set".to_string()));
        }

        let client = openai::Client::from_env();
        
        let mut agent_builder = client.agent(&self.config.model);
        
        if let Some(ref system) = self.config.system_prompt {
            agent_builder = agent_builder.preamble(system);
        }
        
        let agent = agent_builder.build();

        let response: String = agent
            .prompt(prompt)
            .await
            .map_err(|e| Error::ModelProvider(format!("OpenAI error: {}", e)))?;

        Ok(LlmResponse {
            content: response.clone(),
            finish_reason: "stop".to_string(),
            usage: LlmUsage {
                prompt_tokens: (prompt.len() / 4) as u64,
                completion_tokens: (response.len() / 4) as u64,
                total_tokens: ((prompt.len() + response.len()) / 4) as u64,
            },
            tool_calls: None,
        })
    }

    /// Call Anthropic via Rig.
    async fn call_anthropic(&self, prompt: &str) -> Result<LlmResponse> {
        use rig::providers::anthropic;

        // Check env var first to avoid panic
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            return Err(Error::ModelProvider("ANTHROPIC_API_KEY not set".to_string()));
        }

        let client = anthropic::Client::from_env();
        
        let mut agent_builder = client.agent(&self.config.model);
        
        if let Some(ref system) = self.config.system_prompt {
            agent_builder = agent_builder.preamble(system);
        }
        
        let agent = agent_builder.build();

        let response: String = agent
            .prompt(prompt)
            .await
            .map_err(|e| Error::ModelProvider(format!("Anthropic error: {}", e)))?;

        Ok(LlmResponse {
            content: response.clone(),
            finish_reason: "stop".to_string(),
            usage: LlmUsage {
                prompt_tokens: (prompt.len() / 4) as u64,
                completion_tokens: (response.len() / 4) as u64,
                total_tokens: ((prompt.len() + response.len()) / 4) as u64,
            },
            tool_calls: None,
        })
    }
}

#[async_trait]
impl LlmClient for RigLlmClient {
    async fn complete(&self, prompt: &str) -> Result<LlmResponse> {
        tracing::debug!(
            provider = ?self.config.provider,
            model = %self.config.model,
            prompt_len = prompt.len(),
            "Calling LLM"
        );

        match self.config.provider {
            RigProvider::OpenAI => self.call_openai(prompt).await,
            RigProvider::Anthropic => self.call_anthropic(prompt).await,
        }
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<LlmResponse> {
        let prompt = self.build_prompt(messages);
        self.complete(&prompt).await
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        use rig::providers::openai;
        use rig::embeddings::EmbeddingsBuilder;

        if std::env::var("OPENAI_API_KEY").is_err() {
            return Err(Error::ModelProvider("OPENAI_API_KEY not set for embeddings".to_string()));
        }

        let client = openai::Client::from_env();
        let embedding_model = client.embedding_model(openai::TEXT_EMBEDDING_3_SMALL);

        let result = EmbeddingsBuilder::new(embedding_model)
            .document(text)
            .map_err(|e| Error::ModelProvider(format!("Embedding builder error: {}", e)))?
            .build()
            .await
            .map_err(|e| Error::ModelProvider(format!("Embedding error: {}", e)))?;

        // Rig v0.28 returns Vec<(&str, OneOrMany<Embedding>)>
        // OneOrMany can be iterated. Embeddings are f64, convert to f32.
        if let Some((_, one_or_many)) = result.into_iter().next() {
            if let Some(embedding) = one_or_many.into_iter().next() {
                let vec_f32: Vec<f32> = embedding.vec.into_iter().map(|x| x as f32).collect();
                return Ok(vec_f32);
            }
        }
        
        Err(Error::ModelProvider("No embedding returned".to_string()))
    }
}

/// Create a default LLM client based on available API keys.
pub fn create_default_client() -> Result<RigLlmClient> {
    if std::env::var("OPENAI_API_KEY").is_ok() {
        Ok(RigLlmClient::gpt4o_mini())
    } else if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        Ok(RigLlmClient::claude_haiku())
    } else {
        Err(Error::ModelProvider(
            "No API key found. Set OPENAI_API_KEY or ANTHROPIC_API_KEY".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = RigConfig::openai("gpt-4o")
            .with_system_prompt("You are a helpful assistant")
            .with_temperature(0.5);

        assert_eq!(config.provider, RigProvider::OpenAI);
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.system_prompt, Some("You are a helpful assistant".to_string()));
        assert_eq!(config.temperature, Some(0.5));
    }

    #[test]
    fn test_build_prompt() {
        let client = RigLlmClient::gpt4o_mini();
        
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are helpful".to_string(),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                tool_calls: None,
            },
        ];

        let prompt = client.build_prompt(&messages);
        assert!(prompt.contains("System: You are helpful"));
        assert!(prompt.contains("User: Hello"));
    }
}
