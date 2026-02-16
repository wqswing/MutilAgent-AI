#![deny(unused)]
//! Core types, traits, and error definitions for Multiagent.
//!
//! This crate provides the foundational building blocks shared across all layers
//! of the multi-agent system.

pub mod config;
pub mod error;
pub mod events;
pub mod evidence;
pub mod fs_policy;
pub mod mocks;
pub mod template;
pub mod traits;
pub mod types;

pub use error::{Error, Result};
pub use events::*;
pub use traits::*;
pub use types::*;
