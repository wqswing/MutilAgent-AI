//! Context Compression for Long-Running Agent Sessions.
//! 
//! Provides strategies to manage token budgets by intelligently
//! compressing conversation history while preserving essential information.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use multi_agent_core::{Result, traits::{LlmClient, ChatMessage}};

/// Configuration for context compression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Maximum tokens before compression triggers.
    pub max_tokens: usize,
    /// Threshold percentage (0.0-1.0) to trigger compression.
    pub trigger_threshold: f32,
    /// Target percentage (0.0-1.0) after compression.
    pub target_ratio: f32,
    /// Number of recent messages to always preserve.
    pub preserve_recent: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            trigger_threshold: 0.8,
            target_ratio: 0.5,
            preserve_recent: 10,
        }
    }
}

/// Result of a compression operation.
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// Compressed messages.
    pub messages: Vec<ChatMessage>,
    /// Estimated tokens after compression.
    pub estimated_tokens: usize,
    /// Number of messages removed/summarized.
    pub messages_compressed: usize,
}

/// Strategy for context compression.
#[async_trait]
pub trait ContextCompressor: Send + Sync {
    /// Compress a list of messages to fit within token budget.
    async fn compress(
        &self,
        messages: Vec<ChatMessage>,
        config: &CompressionConfig,
    ) -> Result<CompressionResult>;
    
    /// Estimate token count for messages.
    fn estimate_tokens(&self, messages: &[ChatMessage]) -> usize;
    
    /// Check if compression is needed.
    fn needs_compression(&self, messages: &[ChatMessage], config: &CompressionConfig) -> bool {
        let tokens = self.estimate_tokens(messages);
        let threshold = (config.max_tokens as f32 * config.trigger_threshold) as usize;
        tokens > threshold
    }
}

/// Simple truncation strategy - removes oldest messages.
pub struct TruncationCompressor;

impl TruncationCompressor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TruncationCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextCompressor for TruncationCompressor {
    async fn compress(
        &self,
        messages: Vec<ChatMessage>,
        config: &CompressionConfig,
    ) -> Result<CompressionResult> {
        let total = messages.len();
        let target_tokens = (config.max_tokens as f32 * config.target_ratio) as usize;
        
        // Always preserve system message (first) and recent messages
        let system_msg = messages.first().filter(|m| m.role == "system").cloned();
        let preserve_start = if system_msg.is_some() { 1 } else { 0 };
        
        // Keep recent messages
        let keep_recent = total.saturating_sub(config.preserve_recent).max(preserve_start);
        let recent: Vec<_> = messages[keep_recent..].to_vec();
        
        let mut result = Vec::new();
        if let Some(sys) = system_msg {
            result.push(sys);
        }
        
        // Add a summary placeholder
        result.push(ChatMessage {
            role: "system".to_string(),
            content: format!("[Context compressed: {} earlier messages removed]", keep_recent - preserve_start),
            tool_calls: None,
        });
        
        result.extend(recent);
        
        let compressed_count = total - result.len();
        let estimated = self.estimate_tokens(&result);
        
        Ok(CompressionResult {
            messages: result,
            estimated_tokens: estimated.min(target_tokens),
            messages_compressed: compressed_count,
        })
    }
    
    fn estimate_tokens(&self, messages: &[ChatMessage]) -> usize {
        // Rough estimation: ~4 chars per token on average
        messages.iter().map(|m| m.content.len() / 4).sum()
    }
}

/// Summarization strategy - uses LLM to summarize old messages.
pub struct SummarizationCompressor<C: LlmClient> {
    client: C,
}

impl<C: LlmClient> SummarizationCompressor<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<C: LlmClient + 'static> ContextCompressor for SummarizationCompressor<C> {
    async fn compress(
        &self,
        messages: Vec<ChatMessage>,
        config: &CompressionConfig,
    ) -> Result<CompressionResult> {
        let total = messages.len();
        
        // Separate system, old, and recent messages
        let system_msg = messages.first().filter(|m| m.role == "system").cloned();
        let preserve_start = if system_msg.is_some() { 1 } else { 0 };
        let keep_recent = total.saturating_sub(config.preserve_recent).max(preserve_start);
        
        let old_messages = &messages[preserve_start..keep_recent];
        let recent_messages = &messages[keep_recent..];
        
        // Create summary of old messages
        let summary = if !old_messages.is_empty() {
            let old_content: String = old_messages
                .iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n");
            
            let summary_prompt = format!(
                "Summarize the following conversation history in 2-3 concise sentences, \
                 preserving key facts, decisions, and context:\n\n{}",
                old_content
            );
            
            let response = self.client.complete(&summary_prompt).await?;
            Some(response.content)
        } else {
            None
        };
        
        // Build compressed result
        let mut result = Vec::new();
        if let Some(sys) = system_msg {
            result.push(sys);
        }
        
        if let Some(summary_text) = summary {
            result.push(ChatMessage {
                role: "system".to_string(),
                content: format!("[Previous context summary: {}]", summary_text),
                tool_calls: None,
            });
        }
        
        result.extend(recent_messages.iter().cloned());
        
        let compressed_count = old_messages.len();
        let estimated = self.estimate_tokens(&result);
        
        Ok(CompressionResult {
            messages: result,
            estimated_tokens: estimated,
            messages_compressed: compressed_count,
        })
    }
    
    fn estimate_tokens(&self, messages: &[ChatMessage]) -> usize {
        messages.iter().map(|m| m.content.len() / 4).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_messages(count: usize) -> Vec<ChatMessage> {
        let mut msgs = vec![ChatMessage {
            role: "system".to_string(),
            content: "You are a helpful assistant.".to_string(),
            tool_calls: None,
        }];
        
        for i in 0..count {
            msgs.push(ChatMessage {
                role: if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                content: format!("Message {}", i),
                tool_calls: None,
            });
        }
        msgs
    }
    
    #[tokio::test]
    async fn test_truncation_compressor() {
        let compressor = TruncationCompressor::new();
        let messages = make_messages(20);
        
        let config = CompressionConfig {
            preserve_recent: 5,
            ..Default::default()
        };
        
        let result = compressor.compress(messages, &config).await.unwrap();
        
        // Should have: system + summary + 5 recent = 7
        assert_eq!(result.messages.len(), 7);
        assert!(result.messages[1].content.contains("compressed"));
    }
    
    #[test]
    fn test_needs_compression() {
        let compressor = TruncationCompressor::new();
        let config = CompressionConfig {
            max_tokens: 100,
            trigger_threshold: 0.8,
            ..Default::default()
        };
        
        // Small messages - no compression needed
        let small = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
            tool_calls: None,
        }];
        assert!(!compressor.needs_compression(&small, &config));
        
        // Large messages - compression needed
        let large = vec![ChatMessage {
            role: "user".to_string(),
            content: "x".repeat(500),
            tool_calls: None,
        }];
        assert!(compressor.needs_compression(&large, &config));
    }
}
