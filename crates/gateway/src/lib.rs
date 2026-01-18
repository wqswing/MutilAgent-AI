#![deny(unused)]
//! L0 Gateway & Router for Multiagent.
//!
//! This crate provides the HTTP entry point for the system,
//! including semantic caching and intent routing.

pub mod audio;
pub mod router;
pub mod semantic_cache;
pub mod server;
pub mod vision;

pub use audio::{AudioProcessor, AudioFormat, TranscriptionResult};
pub use router::DefaultRouter;
pub use semantic_cache::InMemorySemanticCache;
pub use server::{GatewayServer, GatewayConfig};
pub use vision::{VisionProcessor, ImageInfo};
