//! Core type definitions for Multiagent.
//!
//! This module contains all the fundamental data structures used across
//! the multi-agent system.
//!
//! Broken down into submodules for better maintainability.

pub mod agent;
pub mod intent;
pub mod model;
pub mod refs;
pub mod request;
pub mod session;
pub mod tool;

// Re-export everything to maintain backward compatibility
pub use agent::*;
pub use intent::*;
pub use model::*;
pub use refs::*;
pub use request::*;
pub use session::*;
pub use tool::*;
