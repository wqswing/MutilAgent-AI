#![deny(unused)]
//! L1 Controller for MutilAgent.
//!
//! This crate provides the ReAct loop, DAG orchestration, and SOP engine
//! for executing complex tasks.

pub mod dag;
pub mod persistence;
pub mod react;
pub mod sop;
pub mod context;
pub mod delegation;
pub mod capability;
pub mod memory;
pub mod planning;
pub mod builder;
pub mod parser;
pub mod executor;

pub use persistence::InMemorySessionStore;
pub use mutil_agent_core::traits::SessionStore;
pub use react::{ReActConfig, ReActController, chrono_timestamp};
pub use parser::{ActionParser, ReActAction};
pub use builder::ReActBuilder;
pub use capability::{
    AgentCapability, CompressionCapability, DelegationCapability, McpCapability, SecurityCapability,
    ReflectionCapability,
};
pub use memory::MemoryCapability;
pub use planning::PlanningCapability;
