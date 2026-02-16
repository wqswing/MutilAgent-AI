//! Sandbox integration tests.
//!
//! Tests the full pipeline: Tool → SandboxManager → SandboxEngine (MockSandbox).
//! These tests do NOT require Docker — they use MockSandbox for deterministic behavior.

use serde_json::json;
use std::sync::Arc;

use multi_agent_core::traits::Tool;
use multi_agent_core::types::ToolRiskLevel;
use multi_agent_sandbox::engine::{ExecResult, MockSandbox, SandboxConfig};
use multi_agent_sandbox::tools::{
    SandboxListFilesTool, SandboxManager, SandboxReadFileTool, SandboxShellTool,
    SandboxWriteFileTool,
};

// =============================================================================
// Helpers
// =============================================================================

fn mock_manager(responses: Vec<ExecResult>) -> Arc<SandboxManager> {
    let engine = Arc::new(MockSandbox::new(responses));
    Arc::new(SandboxManager::new(engine, SandboxConfig::default()))
}

fn default_manager() -> Arc<SandboxManager> {
    let engine = Arc::new(MockSandbox::default());
    Arc::new(SandboxManager::new(engine, SandboxConfig::default()))
}

// =============================================================================
// 1. Shell 执行成功
// =============================================================================

#[tokio::test]
async fn test_shell_exec_success() {
    let manager = mock_manager(vec![ExecResult {
        exit_code: 0,
        stdout: "file1.py\nfile2.rs\n".into(),
        stderr: String::new(),
        timed_out: false,
    }]);
    let tool = SandboxShellTool::new(manager);

    let output = tool
        .execute(json!({"command": "ls /workspace"}))
        .await
        .unwrap();

    assert!(output.success, "Shell tool should report success");
    assert!(output.content.contains("file1.py"));
    assert!(output.content.contains("file2.rs"));
}

// =============================================================================
// 2. Shell 执行超时
// =============================================================================

#[tokio::test]
async fn test_shell_exec_timeout() {
    let manager = mock_manager(vec![ExecResult {
        exit_code: -1,
        stdout: "partial output...".into(),
        stderr: "killed".into(),
        timed_out: true,
    }]);
    let tool = SandboxShellTool::new(manager);

    let output = tool
        .execute(json!({"command": "sleep 999", "timeout_secs": 2}))
        .await
        .unwrap();

    assert!(!output.success, "Timed-out command should report failure");
    assert!(
        output.content.contains("timed out"),
        "Error should mention timeout"
    );
}

// =============================================================================
// 3. 文件写入 + 读取全链路
// =============================================================================

#[tokio::test]
async fn test_write_then_read_file_pipeline() {
    let manager = default_manager();
    let write_tool = SandboxWriteFileTool::new(manager.clone());
    let read_tool = SandboxReadFileTool::new(manager.clone());

    // Write a Python script
    let script = "print('Hello from sandbox')";
    let w = write_tool
        .execute(json!({"path": "src/main.py", "content": script}))
        .await
        .unwrap();
    assert!(w.success, "Write tool should succeed");
    assert!(w.content.contains("src/main.py"));

    // Read it back
    let r = read_tool
        .execute(json!({"path": "src/main.py"}))
        .await
        .unwrap();
    assert!(r.success, "Read tool should succeed");
    assert_eq!(r.content, script, "Content should be unchanged");

    // Read a non-existent file
    let err = read_tool.execute(json!({"path": "nonexistent.txt"})).await;
    assert!(err.is_err(), "Reading non-existent file should error");
}

// =============================================================================
// 4. 风险等级标记
// =============================================================================

#[tokio::test]
async fn test_risk_levels() {
    let manager = default_manager();

    let shell = SandboxShellTool::new(manager.clone());
    let write = SandboxWriteFileTool::new(manager.clone());
    let read = SandboxReadFileTool::new(manager.clone());
    let list = SandboxListFilesTool::new(manager);

    assert!(
        matches!(shell.risk_level(), ToolRiskLevel::High),
        "Shell tool should be High risk"
    );
    assert!(
        matches!(write.risk_level(), ToolRiskLevel::Medium),
        "Write tool should be Medium risk"
    );
    // ReadFile and ListFiles default to Low
    assert!(
        matches!(read.risk_level(), ToolRiskLevel::Low),
        "Read tool should be Low risk"
    );
    assert!(
        matches!(list.risk_level(), ToolRiskLevel::Low),
        "List tool should be Low risk"
    );
}

// =============================================================================
// 5. 沙箱生命周期（create → reuse → teardown → recreate）
// =============================================================================

#[tokio::test]
async fn test_sandbox_lifecycle() {
    let manager = default_manager();

    // First get_or_create: creates a new sandbox
    let id1 = manager.get_or_create().await.unwrap();

    // Second get_or_create: reuses the same sandbox
    let id2 = manager.get_or_create().await.unwrap();
    assert_eq!(id1.0, id2.0, "Should reuse the same sandbox");

    // Teardown
    manager.teardown().await.unwrap();

    // After teardown, get_or_create should create a new one
    let id3 = manager.get_or_create().await.unwrap();
    assert_ne!(id1.0, id3.0, "Should create a new sandbox after teardown");

    // Verify availability
    assert!(
        manager.is_available().await,
        "MockSandbox should be available"
    );
}
