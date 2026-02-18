//! Sandbox execution engine.
//!
//! This module provides the `SandboxEngine` trait and a Docker-based implementation
//! using the `bollard` crate. The sandbox creates isolated Linux containers with
//! strict resource limits, no host network access, and a read-only root filesystem.

use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use multi_agent_core::Result;

// =============================================================================
// Sandbox Types
// =============================================================================

/// Unique identifier for a sandbox instance.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SandboxId(pub String);

impl std::fmt::Display for SandboxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Network isolation profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkProfile {
    /// No network access (default).
    None,
    /// Full access to host network (dangerous).
    Host,
    /// Bridge network (standard Docker networking).
    Bridge,
    /// Custom network name.
    Custom(String),
}

/// Configuration for creating a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Docker image to use (default: "opencoordex-sandbox:latest").
    pub image: String,
    /// Maximum memory in bytes (default: 512MB).
    pub memory_limit: i64,
    /// CPU period/quota (default: 1 core equivalent).
    pub cpu_quota: i64,
    /// Default execution timeout.
    pub default_timeout: Duration,
    /// Network isolation profile.
    pub network_profile: NetworkProfile,
    /// Working directory inside the container.
    pub workdir: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            image: "opencoordex-sandbox:latest".to_string(),
            memory_limit: 512 * 1024 * 1024, // 512MB
            cpu_quota: 100_000,              // 1 CPU core
            default_timeout: Duration::from_secs(30),
            network_profile: NetworkProfile::None,
            workdir: "/workspace".to_string(),
        }
    }
}

/// Result of executing a command in the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Exit code of the command.
    pub exit_code: i64,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Whether the command timed out.
    pub timed_out: bool,
}

impl ExecResult {
    /// Whether the execution was successful (exit code 0, no timeout).
    pub fn success(&self) -> bool {
        self.exit_code == 0 && !self.timed_out
    }
}

// =============================================================================
// Sandbox Engine Trait
// =============================================================================

/// Trait for sandbox execution backends.
///
/// Implementations provide isolated environments for running untrusted code.
/// The default implementation uses Docker containers via `bollard`.
#[async_trait]
pub trait SandboxEngine: Send + Sync {
    /// Create a new sandbox container.
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId>;

    /// Execute a command inside the sandbox.
    async fn exec(&self, id: &SandboxId, command: &str, timeout: Duration) -> Result<ExecResult>;

    /// Write a file into the sandbox at the given path (relative to workdir).
    async fn write_file(&self, id: &SandboxId, path: &str, content: &[u8]) -> Result<()>;

    /// Read a file from the sandbox at the given path (relative to workdir).
    async fn read_file(&self, id: &SandboxId, path: &str) -> Result<Vec<u8>>;

    /// Destroy the sandbox and clean up resources.
    async fn destroy(&self, id: &SandboxId) -> Result<()>;

    /// Check if the sandbox backend is available (e.g., Docker daemon running).
    async fn is_available(&self) -> bool;
}

// =============================================================================
// Docker Sandbox Implementation
// =============================================================================

/// Docker-based sandbox engine using the `bollard` crate.
///
/// Creates isolated containers with:
/// - No host network access (by default)
/// - Read-only root filesystem (writable `/workspace` only)
/// - Memory and CPU limits
/// - Non-root user execution
/// - Execution timeout enforcement
pub struct DockerSandbox {
    docker: bollard::Docker,
    event_emitter: Option<Arc<dyn multi_agent_core::traits::EventEmitter>>,
}

impl DockerSandbox {
    /// Create a new Docker sandbox engine connecting to the local Docker daemon.
    pub fn new() -> Result<Self> {
        let docker = bollard::Docker::connect_with_local_defaults().map_err(|e| {
            multi_agent_core::Error::internal(format!(
                "Failed to connect to Docker daemon: {}. Is Docker running?",
                e
            ))
        })?;
        Ok(Self {
            docker,
            event_emitter: None,
        })
    }

    /// Set an event emitter for auditing sandbox operations.
    pub fn with_event_emitter(
        mut self,
        emitter: Arc<dyn multi_agent_core::traits::EventEmitter>,
    ) -> Self {
        self.event_emitter = Some(emitter);
        self
    }

    /// Create from an existing bollard Docker client (for testing).
    pub fn from_client(docker: bollard::Docker) -> Self {
        Self {
            docker,
            event_emitter: None,
        }
    }
}

#[async_trait]
impl SandboxEngine for DockerSandbox {
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId> {
        use bollard::container::{Config, CreateContainerOptions};
        use bollard::models::{HostConfig, Mount, MountTypeEnum};

        let sandbox_id = format!("msa-sandbox-{}", uuid::Uuid::new_v4());

        let host_config = HostConfig {
            memory: Some(config.memory_limit),
            cpu_quota: Some(config.cpu_quota),
            cpu_period: Some(100_000), // standard 100ms period
            network_mode: match &config.network_profile {
                NetworkProfile::None => Some("none".to_string()),
                NetworkProfile::Host => Some("host".to_string()),
                NetworkProfile::Bridge => Some("bridge".to_string()),
                NetworkProfile::Custom(name) => Some(name.clone()),
            },
            // Mount a tmpfs at /workspace for writable scratch space
            mounts: Some(vec![Mount {
                target: Some(config.workdir.clone()),
                typ: Some(MountTypeEnum::TMPFS),
                tmpfs_options: Some(bollard::models::MountTmpfsOptions {
                    size_bytes: Some(config.memory_limit / 2), // half of memory limit
                    ..Default::default()
                }),
                ..Default::default()
            }]),
            readonly_rootfs: Some(true),
            // Drop all capabilities by default
            cap_drop: Some(vec!["ALL".to_string()]),
            // Security: no privilege escalation
            security_opt: Some(vec!["no-new-privileges:true".to_string()]),
            // Resource limits: prevent fork bombs and too many open files
            pids_limit: Some(100),
            ulimits: Some(vec![bollard::models::ResourcesUlimits {
                name: Some("nofile".to_string()),
                soft: Some(1024),
                hard: Some(2048),
            }]),
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config.image.clone()),
            working_dir: Some(config.workdir.clone()),
            user: Some("agent".to_string()), // non-root
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            host_config: Some(host_config),
            labels: Some(std::collections::HashMap::from([(
                "managed-by".to_string(),
                "opencoordex-sandbox".to_string(),
            )])),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: &sandbox_id,
            platform: None,
        };

        self.docker
            .create_container(Some(options), container_config)
            .await
            .map_err(|e| {
                multi_agent_core::Error::internal(format!(
                    "Failed to create sandbox container: {}",
                    e
                ))
            })?;

        // Start the container
        self.docker
            .start_container::<String>(&sandbox_id, None)
            .await
            .map_err(|e| {
                multi_agent_core::Error::internal(format!(
                    "Failed to start sandbox container: {}",
                    e
                ))
            })?;

        tracing::info!(sandbox_id = %sandbox_id, image = %config.image, "Sandbox container created and started");

        Ok(SandboxId(sandbox_id))
    }

    async fn exec(&self, id: &SandboxId, command: &str, timeout: Duration) -> Result<ExecResult> {
        use bollard::exec::{CreateExecOptions, StartExecResults};

        let exec_options = CreateExecOptions {
            cmd: Some(vec!["sh", "-c", command]),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/workspace"),
            user: Some("agent"),
            ..Default::default()
        };

        let exec = self
            .docker
            .create_exec(&id.0, exec_options)
            .await
            .map_err(|e| {
                multi_agent_core::Error::tool_execution(format!(
                    "Failed to create exec in sandbox: {}",
                    e
                ))
            })?;

        let start_result = self.docker.start_exec(&exec.id, None).await.map_err(|e| {
            multi_agent_core::Error::tool_execution(format!(
                "Failed to start exec in sandbox: {}",
                e
            ))
        })?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = start_result {
            use futures::StreamExt;

            let collect_future = async {
                while let Some(msg) = output.next().await {
                    match msg {
                        Ok(bollard::container::LogOutput::StdOut { message }) => {
                            stdout.push_str(&String::from_utf8_lossy(&message));
                        }
                        Ok(bollard::container::LogOutput::StdErr { message }) => {
                            stderr.push_str(&String::from_utf8_lossy(&message));
                        }
                        Ok(_) => {} // ignore stdin logs
                        Err(e) => {
                            stderr.push_str(&format!("\n[sandbox error: {}]", e));
                            break;
                        }
                    }
                }
            };

            // Apply timeout
            match tokio::time::timeout(timeout, collect_future).await {
                Ok(()) => {} // completed normally
                Err(_) => {
                    tracing::warn!(sandbox = %id, command = %command, "Sandbox exec timed out");
                    return Ok(ExecResult {
                        exit_code: -1,
                        stdout,
                        stderr: format!("{}\n[Execution timed out after {:?}]", stderr, timeout),
                        timed_out: true,
                    });
                }
            }
        }

        // Get exit code
        let inspect = self.docker.inspect_exec(&exec.id).await.map_err(|e| {
            multi_agent_core::Error::tool_execution(format!("Failed to inspect exec result: {}", e))
        })?;

        let exit_code = inspect.exit_code.unwrap_or(-1);

        let exec_result = ExecResult {
            exit_code,
            stdout,
            stderr,
            timed_out: false,
        };

        // Audit: Tool Exec Finished
        if let Some(ref emitter) = self.event_emitter {
            let payload = multi_agent_core::events::ToolExecPayload {
                tool_name: "sandbox_exec".to_string(),
                input: Some(serde_json::json!({ "command": command })),
                output: Some(exec_result.stdout.clone()),
                duration_ms: None,
                error: if exec_result.success() {
                    None
                } else {
                    Some(exec_result.stderr.clone())
                },
            };
            emitter
                .emit(
                    multi_agent_core::events::EventEnvelope::new(
                        multi_agent_core::events::EventType::ToolExecFinished,
                        serde_json::to_value(payload).unwrap_or_default(),
                    )
                    .with_actor("sandbox-engine"),
                )
                .await;
        }

        Ok(exec_result)
    }

    async fn write_file(&self, id: &SandboxId, path: &str, content: &[u8]) -> Result<()> {
        // Use `docker exec` to write the file via base64 piping
        // This avoids needing tar archives for small files
        let b64 = base64::engine::general_purpose::STANDARD.encode(content);
        let command = format!(
            "echo '{}' | base64 -d > /workspace/{}",
            b64,
            path.trim_start_matches('/')
        );

        let result = self.exec(id, &command, Duration::from_secs(10)).await?;

        // Audit: FS Write
        if let Some(ref emitter) = self.event_emitter {
            let payload = multi_agent_core::events::FsPayload {
                path: path.to_string(),
                operation: "write".to_string(),
                size_bytes: Some(content.len() as u64),
                success: result.success(),
                error: if result.success() {
                    None
                } else {
                    Some(result.stderr.clone())
                },
            };
            emitter
                .emit(
                    multi_agent_core::events::EventEnvelope::new(
                        multi_agent_core::events::EventType::FsWrite,
                        serde_json::to_value(payload).unwrap_or_default(),
                    )
                    .with_actor("sandbox-engine"),
                )
                .await;
        }

        if !result.success() {
            return Err(multi_agent_core::Error::tool_execution(format!(
                "Failed to write file '{}' in sandbox: {}",
                path, result.stderr
            )));
        }

        Ok(())
    }

    async fn read_file(&self, id: &SandboxId, path: &str) -> Result<Vec<u8>> {
        let command = format!("cat /workspace/{}", path.trim_start_matches('/'));
        let result = self.exec(id, &command, Duration::from_secs(10)).await?;

        // Audit: FS Read
        if let Some(ref emitter) = self.event_emitter {
            let payload = multi_agent_core::events::FsPayload {
                path: path.to_string(),
                operation: "read".to_string(),
                size_bytes: if result.success() {
                    Some(result.stdout.len() as u64)
                } else {
                    None
                },
                success: result.success(),
                error: if result.success() {
                    None
                } else {
                    Some(result.stderr.clone())
                },
            };
            emitter
                .emit(
                    multi_agent_core::events::EventEnvelope::new(
                        multi_agent_core::events::EventType::FsRead,
                        serde_json::to_value(payload).unwrap_or_default(),
                    )
                    .with_actor("sandbox-engine"),
                )
                .await;
        }

        if !result.success() {
            return Err(multi_agent_core::Error::tool_execution(format!(
                "Failed to read file '{}' in sandbox: {}",
                path, result.stderr
            )));
        }

        Ok(result.stdout.into_bytes())
    }

    async fn destroy(&self, id: &SandboxId) -> Result<()> {
        use bollard::container::{RemoveContainerOptions, StopContainerOptions};

        // Stop the container (with 5s grace period)
        let _ = self
            .docker
            .stop_container(&id.0, Some(StopContainerOptions { t: 5 }))
            .await;

        // Remove the container
        self.docker
            .remove_container(
                &id.0,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| {
                multi_agent_core::Error::internal(format!(
                    "Failed to remove sandbox container: {}",
                    e
                ))
            })?;

        tracing::info!(sandbox_id = %id, "Sandbox container destroyed");
        Ok(())
    }

    async fn is_available(&self) -> bool {
        self.docker.ping().await.is_ok()
    }
}

// =============================================================================
// Mock Sandbox (for testing without Docker)
// =============================================================================

/// In-memory mock sandbox for unit testing.
#[derive(Default)]
pub struct MockSandbox {
    pub exec_responses: std::sync::Arc<tokio::sync::Mutex<Vec<ExecResult>>>,
    pub files: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

impl MockSandbox {
    /// Create a mock sandbox with predefined exec responses.
    pub fn new(responses: Vec<ExecResult>) -> Self {
        Self {
            exec_responses: std::sync::Arc::new(tokio::sync::Mutex::new(responses)),
            files: Default::default(),
        }
    }
}

#[async_trait]
impl SandboxEngine for MockSandbox {
    async fn create(&self, _config: &SandboxConfig) -> Result<SandboxId> {
        Ok(SandboxId(format!("mock-sandbox-{}", uuid::Uuid::new_v4())))
    }

    async fn exec(
        &self,
        _id: &SandboxId,
        _command: &str,
        _timeout: Duration,
    ) -> Result<ExecResult> {
        let mut responses = self.exec_responses.lock().await;
        if responses.is_empty() {
            Ok(ExecResult {
                exit_code: 0,
                stdout: "[mock] command executed".to_string(),
                stderr: String::new(),
                timed_out: false,
            })
        } else {
            Ok(responses.remove(0))
        }
    }

    async fn write_file(&self, _id: &SandboxId, path: &str, content: &[u8]) -> Result<()> {
        self.files
            .lock()
            .await
            .insert(path.to_string(), content.to_vec());
        Ok(())
    }

    async fn read_file(&self, _id: &SandboxId, path: &str) -> Result<Vec<u8>> {
        self.files.lock().await.get(path).cloned().ok_or_else(|| {
            multi_agent_core::Error::tool_execution(format!(
                "File not found in mock sandbox: {}",
                path
            ))
        })
    }

    async fn destroy(&self, _id: &SandboxId) -> Result<()> {
        Ok(())
    }

    async fn is_available(&self) -> bool {
        true
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_config_defaults() {
        let config = SandboxConfig::default();
        assert_eq!(config.image, "opencoordex-sandbox:latest");
        assert_eq!(config.memory_limit, 512 * 1024 * 1024);
        assert!(matches!(config.network_profile, NetworkProfile::None));
        assert_eq!(config.workdir, "/workspace");
    }

    #[tokio::test]
    async fn test_exec_result_success() {
        let result = ExecResult {
            exit_code: 0,
            stdout: "hello".into(),
            stderr: String::new(),
            timed_out: false,
        };
        assert!(result.success());

        let timeout_result = ExecResult {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
        };
        assert!(!timeout_result.success());
    }

    #[tokio::test]
    async fn test_mock_sandbox_lifecycle() {
        let mock = MockSandbox::new(vec![ExecResult {
            exit_code: 0,
            stdout: "Hello Sovereign World".into(),
            stderr: String::new(),
            timed_out: false,
        }]);

        let config = SandboxConfig::default();
        let id = mock.create(&config).await.unwrap();

        // Write file
        mock.write_file(&id, "test.txt", b"hello world")
            .await
            .unwrap();

        // Read file
        let content = mock.read_file(&id, "test.txt").await.unwrap();
        assert_eq!(content, b"hello world");

        // Execute command
        let result = mock
            .exec(&id, "echo Hello", Duration::from_secs(5))
            .await
            .unwrap();
        assert!(result.success());
        assert_eq!(result.stdout, "Hello Sovereign World");

        // Destroy
        mock.destroy(&id).await.unwrap();
    }

    #[tokio::test]
    async fn test_mock_sandbox_file_not_found() {
        let mock = MockSandbox::default();
        let id = SandboxId("test".into());
        let result = mock.read_file(&id, "nonexistent.txt").await;
        assert!(result.is_err());
    }
}
