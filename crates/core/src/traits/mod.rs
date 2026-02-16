//! Core traits for Multiagent.
//!
//! Traits are organized by architectural layer:
//! - `gateway`: L0 Gateway traits (IntentRouter, SemanticCache)
//! - `controller`: L1 Controller traits (Controller, SopEngine, SessionStore)
//! - `skills`: L2 Skills traits (Tool, ToolRegistry, McpAdapter)
//! - `store`: L3 Store traits (ArtifactStore, MemoryStore)
//! - `governance`: L4 Governance traits (BudgetController, SecurityProxy)
//! - `llm`: L-M Model Gateway traits (LlmClient, ModelSelector)
//! - `state_store`: Stateless architecture traits (StateStore, DistributedRateLimiter)

pub mod controller;
pub mod events;
pub mod gateway;
pub mod governance;
pub mod llm;
pub mod skills;
pub mod state_store;
pub mod store;

// Re-export all traits for backward compatibility
pub use controller::*;
pub use events::*;
pub use gateway::*;
pub use governance::*;
pub use llm::*;
pub use skills::*;
pub use state_store::*;
pub use store::*;
