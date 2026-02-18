#![deny(unused)]
//! Sovereign Sandbox for OpenCoordex.
//!
//! This crate provides an isolated execution environment for the agent using
//! Docker containers. All code execution, file I/O, and shell commands are
//! routed through the sandbox, ensuring the host system is never directly affected.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────┐
//! │  L1: Controller (ReAct Loop)           │
//! │    ↓ calls tool                        │
//! ├────────────────────────────────────────┤
//! │  L2: Skills (SandboxShellTool, etc.)   │
//! │    ↓ delegates to SandboxManager       │
//! ├────────────────────────────────────────┤
//! │  Sandbox Engine (DockerSandbox)        │
//! │    ↓ Docker API via bollard            │
//! ├────────────────────────────────────────┤
//! │  Docker Container (isolated)           │
//! │    /workspace  (tmpfs, writable)       │
//! │    No host network, no root, no caps   │
//! └────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use multi_agent_sandbox::{DockerSandbox, SandboxManager, SandboxConfig};
//! use multi_agent_sandbox::tools::{SandboxShellTool, SandboxWriteFileTool};
//!
//! let engine = Arc::new(DockerSandbox::new()?);
//! let manager = Arc::new(SandboxManager::new(engine, SandboxConfig::default()));
//!
//! // Register tools
//! registry.register(Box::new(SandboxShellTool::new(manager.clone()))).await?;
//! registry.register(Box::new(SandboxWriteFileTool::new(manager.clone()))).await?;
//! ```

pub mod engine;
pub mod tools;

pub use engine::{DockerSandbox, ExecResult, MockSandbox, SandboxConfig, SandboxEngine, SandboxId};
pub use tools::{
    SandboxListFilesTool, SandboxManager, SandboxReadFileTool, SandboxShellTool,
    SandboxWriteFileTool,
};
