//! governed network tools.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use multi_agent_core::{traits::Tool, types::ToolOutput, Error, Result};
use multi_agent_core::config::SafetyConfig;
use multi_agent_governance::network::NetworkPolicy;
use futures::StreamExt;
use sha2::{Digest, Sha256};



use tokio::sync::RwLock;

// Local fetch_with_policy removed in favor of multi_agent_governance::network::fetch_with_policy

/// Tool for fetching text/json content from a URL (GET/POST).
#[derive(Clone)]
pub struct FetchTool {
    policy: Arc<RwLock<NetworkPolicy>>,
    safety: SafetyConfig,
    client: reqwest::Client,
}

impl FetchTool {
    pub fn new(policy: Arc<RwLock<NetworkPolicy>>, safety: SafetyConfig) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
            
        Self {
            policy,
            safety,
            client,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FetchArgs {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

#[async_trait]
impl Tool for FetchTool {
    // ... name/desc/params ... (skip unchanged)
    fn name(&self) -> &str {
        "fetch"
    }

    fn description(&self) -> &str {
        "Fetch text or JSON content from a URL (HTTP GET/POST). Subject to network policy."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(FetchArgs)).unwrap()
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput> {
        let args: FetchArgs = serde_json::from_value(args)
            .map_err(|e| Error::tool_execution(format!("Invalid arguments: {}", e)))?;

        let url = url::Url::parse(&args.url)
            .map_err(|e| Error::tool_execution(format!("Invalid URL: {}", e)))?;

        let method = args.method.to_uppercase().parse::<reqwest::Method>()
            .map_err(|_| Error::tool_execution(format!("Invalid HTTP method: {}", args.method)))?;

        let mut headers = reqwest::header::HeaderMap::new();
        for (k, v) in args.headers {
            if let Ok(k) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(v) = reqwest::header::HeaderValue::from_str(&v) {
                    headers.insert(k, v);
                }
            }
        }

        // Use helper
        let policy_version = {
            self.policy.read().await.version.clone()
        };

        let policy = self.policy.read().await.clone();

        // Use helper
        let resp = multi_agent_governance::network::fetch_with_policy(
            &self.client,
            &policy,
            &self.safety,
            method,
            url,
            Some(&headers),
            args.body.as_ref()
        ).await.map_err(|e| Error::tool_execution(e.to_string()))?;
        
        let status = resp.status();
        
        // Read body with limit
        let mut stream = resp.bytes_stream();
        let mut buffer = Vec::new();
        let mut total_size = 0;
        let limit = self.safety.max_download_size_bytes;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::tool_execution(format!("Download failed: {}", e)))?;
            total_size += chunk.len() as u64;
            if total_size > limit {
                return Err(Error::tool_execution(format!("Response size exceeded limit ({} bytes)", limit)));
            }
            buffer.extend_from_slice(&chunk);
        }
        
        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let body_hash = format!("{:x}", hasher.finalize());

        let content = String::from_utf8(buffer)
             .map_err(|e| Error::tool_execution(format!("Invalid UTF-8 content: {}", e)))?;

        if !status.is_success() {
             return Ok(ToolOutput::text(format!("HTTP Error {}: {}", status, content))
                .with_data(serde_json::json!({
                    "policy_version": policy_version,
                    "status": status.as_u16(),
                    "url": args.url,
                    "body_hash": body_hash
                })));
        }
        
        Ok(ToolOutput::text(content).with_data(serde_json::json!({
            "policy_version": policy_version,
            "status": status.as_u16(),
            "url": args.url,
            "body_hash": body_hash
        })))
    }
}

// ... DownloadTool ...

/// Tool for downloading files to the sandbox.
#[derive(Clone)]
pub struct DownloadTool {
    policy: Arc<RwLock<NetworkPolicy>>,
    safety: SafetyConfig,
    client: reqwest::Client,
    sandbox_manager: Arc<multi_agent_sandbox::SandboxManager>,
}

impl DownloadTool {
    pub fn new(
        policy: Arc<RwLock<NetworkPolicy>>,
        safety: SafetyConfig,
        sandbox_manager: Arc<multi_agent_sandbox::SandboxManager>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            policy,
            safety,
            client,
            sandbox_manager,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DownloadArgs {
    pub url: String,
    pub destination_path: String,
}

#[async_trait]
impl Tool for DownloadTool {
    // ... name/desc/params ... (skip unchanged)
    fn name(&self) -> &str {
        "download"
    }

    fn description(&self) -> &str {
        "Download a file from a URL and save it to the sandbox workspace."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(DownloadArgs)).unwrap()
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput> {
        let args: DownloadArgs = serde_json::from_value(args)
            .map_err(|e| Error::tool_execution(format!("Invalid arguments: {}", e)))?;

        let url = url::Url::parse(&args.url)
            .map_err(|e| Error::tool_execution(format!("Invalid URL: {}", e)))?;

        let policy_version = {
            self.policy.read().await.version.clone()
        };

        let policy = self.policy.read().await.clone();

        // Download is GET, no body, no custom headers from args (for now)
        let resp = multi_agent_governance::network::fetch_with_policy(
            &self.client,
            &policy,
            &self.safety,
            reqwest::Method::GET,
            url,
            None,
            None
        ).await.map_err(|e| Error::tool_execution(e.to_string()))?;

        if !resp.status().is_success() {
             return Err(Error::tool_execution(format!("HTTP Error {}", resp.status())));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = Vec::new();
        let mut total_size = 0;
        let limit = self.safety.max_download_size_bytes;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::tool_execution(format!("Download failed: {}", e)))?;
            total_size += chunk.len() as u64;
            if total_size > limit {
                 return Err(Error::tool_execution(format!("Download size limit exceeded ({} bytes)", limit)));
            }
            buffer.extend_from_slice(&chunk);
        }

        let sandbox_id: multi_agent_sandbox::SandboxId = self.sandbox_manager.get_or_create().await
             .map_err(|e| Error::tool_execution(format!("Failed to get sandbox: {}", e)))?;
        
        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let body_hash = format!("{:x}", hasher.finalize());

        self.sandbox_manager.engine()
            .write_file(&sandbox_id, &args.destination_path, &buffer).await
            .map_err(|e| Error::tool_execution(format!("Failed to write to sandbox: {}", e)))?;

        Ok(ToolOutput::text(format!("Successfully downloaded {} bytes to {}", buffer.len(), args.destination_path))
            .with_data(serde_json::json!({
                "policy_version": policy_version,
                "url": args.url,
                "destination": args.destination_path,
                "bytes": buffer.len(),
                "body_hash": body_hash
            }))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multi_agent_governance::network::NetworkPolicy;

    use multi_agent_core::config::SafetyConfig;

    #[tokio::test]
    async fn test_fetch_tool_policy_deny() {
        let policy = Arc::new(tokio::sync::RwLock::new(NetworkPolicy::default())); // Defaults deny everything
        let tool = FetchTool::new(policy, SafetyConfig::default());

        let args = serde_json::json!({
            "url": "https://google.com"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Network policy denied access"));
    }

    #[tokio::test]
    async fn test_fetch_tool_policy_allow() {
        let policy = Arc::new(tokio::sync::RwLock::new(NetworkPolicy::new(
            vec!["google.com".to_string()],
            vec![],
            vec![80, 443],
        )));
        let tool = FetchTool::new(policy, SafetyConfig::default());

        // We expect a network error (since we have no internet or it might fail),
        // but NOT a policy error.
        let args = serde_json::json!({
            "url": "https://google.com"
        });

        let result = tool.execute(args).await;
        // It might succeed if we have net access, or fail with connection error.
        // We just check it didn't fail with "Network policy denied access".
        if let Err(e) = result {
            assert!(
                !e.to_string().contains("Network policy denied access"),
                "Should have passed policy check. Error was: {}",
                e
            );
        }
    }
}
