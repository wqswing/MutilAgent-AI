//! Core type definitions for Multiagent.
//!
//! This module contains all the fundamental data structures used across
//! the multi-agent system.
//! 
//! Broken down into submodules for better maintainability.

pub mod refs;
pub mod request;
pub mod intent;
pub mod tool;
pub mod agent;
pub mod session;
pub mod model;

// Re-export everything to maintain backward compatibility
pub use refs::*;
pub use request::*;
pub use intent::*;
pub use tool::*;
pub use agent::*;
pub use session::*;
pub use model::*;
