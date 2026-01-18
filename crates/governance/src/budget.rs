//! Token budget controller.

use async_trait::async_trait;
use dashmap::DashMap;
use std::time::{Duration, Instant};

use multi_agent_core::{
    traits::BudgetController,
    types::TokenUsage,
    Error, Result,
};

/// Budget entry for a session.
#[derive(Debug, Clone)]
struct BudgetEntry {
    /// Token usage tracking.
    usage: TokenUsage,
    /// Reserved tokens (not yet consumed).
    reserved: u64,
    /// Last update time.
    last_update: Instant,
}

impl BudgetEntry {
    fn new(limit: u64) -> Self {
        Self {
            usage: TokenUsage::with_budget(limit),
            reserved: 0,
            last_update: Instant::now(),
        }
    }

    fn is_exceeded(&self) -> bool {
        self.usage.total_tokens + self.reserved >= self.usage.budget_limit
    }

    fn remaining(&self) -> u64 {
        self.usage
            .budget_limit
            .saturating_sub(self.usage.total_tokens + self.reserved)
    }
}

/// Token budget controller using in-memory storage.
pub struct TokenBudgetController {
    /// Budget entries by session ID.
    budgets: DashMap<String, BudgetEntry>,
    /// Default budget limit.
    default_limit: u64,
    /// Entry expiration time.
    expiration: Duration,
}

impl TokenBudgetController {
    /// Create a new budget controller.
    pub fn new(default_limit: u64) -> Self {
        Self {
            budgets: DashMap::new(),
            default_limit,
            expiration: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Set the expiration time.
    pub fn with_expiration(mut self, expiration: Duration) -> Self {
        self.expiration = expiration;
        self
    }

    /// Get or create a budget entry.
    fn get_or_create(&self, session_id: &str) -> dashmap::mapref::one::RefMut<'_, String, BudgetEntry> {
        self.budgets
            .entry(session_id.to_string())
            .or_insert_with(|| BudgetEntry::new(self.default_limit))
    }

    /// Clean up expired entries.
    pub fn cleanup(&self) {
        self.budgets.retain(|_, v| v.last_update.elapsed() < self.expiration);
    }

    /// Get total active sessions.
    pub fn active_sessions(&self) -> usize {
        self.budgets.len()
    }
}

#[async_trait]
impl BudgetController for TokenBudgetController {
    async fn reserve(&self, session_id: &str, tokens: u64) -> Result<()> {
        let mut entry = self.get_or_create(session_id);

        if entry.remaining() < tokens {
            return Err(Error::BudgetExceeded {
                used: entry.usage.total_tokens + entry.reserved,
                limit: entry.usage.budget_limit,
            });
        }

        entry.reserved += tokens;
        entry.last_update = Instant::now();

        tracing::debug!(
            session_id = session_id,
            tokens = tokens,
            remaining = entry.remaining(),
            "Reserved tokens"
        );

        Ok(())
    }

    async fn release(&self, session_id: &str, tokens: u64) -> Result<()> {
        if let Some(mut entry) = self.budgets.get_mut(session_id) {
            entry.reserved = entry.reserved.saturating_sub(tokens);
            entry.last_update = Instant::now();

            tracing::debug!(
                session_id = session_id,
                tokens = tokens,
                reserved = entry.reserved,
                "Released tokens"
            );
        }

        Ok(())
    }

    async fn record_usage(&self, session_id: &str, prompt: u64, completion: u64) -> Result<()> {
        let mut entry = self.get_or_create(session_id);

        entry.usage.add(prompt, completion);
        // Release reserved tokens as they're consumed
        entry.reserved = entry.reserved.saturating_sub(prompt + completion);
        entry.last_update = Instant::now();

        tracing::debug!(
            session_id = session_id,
            prompt = prompt,
            completion = completion,
            total = entry.usage.total_tokens,
            "Recorded token usage"
        );

        Ok(())
    }

    async fn remaining(&self, session_id: &str) -> Result<u64> {
        Ok(self
            .budgets
            .get(session_id)
            .map(|e| e.remaining())
            .unwrap_or(self.default_limit))
    }

    async fn is_exceeded(&self, session_id: &str) -> Result<bool> {
        Ok(self
            .budgets
            .get(session_id)
            .map(|e| e.is_exceeded())
            .unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reserve_and_record() {
        let controller = TokenBudgetController::new(10000);

        // Reserve tokens
        controller.reserve("session1", 5000).await.unwrap();
        assert_eq!(controller.remaining("session1").await.unwrap(), 5000);

        // Record actual usage (4000 tokens used, 1000 of reservation remains)
        // Remaining = 10000 - 4000 used - 1000 reserved = 5000
        controller.record_usage("session1", 3000, 1000).await.unwrap();
        assert_eq!(controller.remaining("session1").await.unwrap(), 5000);
    }

    #[tokio::test]
    async fn test_budget_exceeded() {
        let controller = TokenBudgetController::new(1000);

        // Try to reserve more than available
        let result = controller.reserve("session1", 2000).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_release() {
        let controller = TokenBudgetController::new(10000);

        controller.reserve("session1", 5000).await.unwrap();
        assert_eq!(controller.remaining("session1").await.unwrap(), 5000);

        controller.release("session1", 3000).await.unwrap();
        assert_eq!(controller.remaining("session1").await.unwrap(), 8000);
    }
}
