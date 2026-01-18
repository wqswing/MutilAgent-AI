#![deny(unused)]
//! Core types, traits, and error definitions for MutilAgent.
//!
//! This crate provides the foundational building blocks shared across all layers
//! of the multi-agent system.

pub mod error;
pub mod traits;
pub mod types;
pub mod template;
pub mod evidence;
pub mod mocks;

pub use error::{Error, Result};
pub use traits::*;
pub use types::*;
