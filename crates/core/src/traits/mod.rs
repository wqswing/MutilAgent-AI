//! Core traits for MutilAgent.
//!
//! Traits are organized by architectural layer:
//! - `gateway`: L0 Gateway traits (IntentRouter, SemanticCache)
//! - `controller`: L1 Controller traits (Controller, SopEngine, SessionStore)
//! - `skills`: L2 Skills traits (Tool, ToolRegistry, McpAdapter)
//! - `store`: L3 Store traits (ArtifactStore, MemoryStore)
//! - `governance`: L4 Governance traits (BudgetController, SecurityProxy)
//! - `llm`: L-M Model Gateway traits (LlmClient, ModelSelector)

pub mod gateway;
pub mod controller;
pub mod skills;
pub mod store;
pub mod governance;
pub mod llm;

// Re-export all traits for backward compatibility
pub use gateway::*;
pub use controller::*;
pub use skills::*;
pub use store::*;
pub use governance::*;
pub use llm::*;
