//! governed network tools.

use std::sync::Arc;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use multi_agent_core::{
    traits::Tool,
    types::ToolOutput,
    Error,
    Result,
};
use multi_agent_governance::network::NetworkPolicy;

/// Tool for fetching text/json content from a URL (GET/POST).
#[derive(Clone)]
pub struct FetchTool {
    policy: Arc<NetworkPolicy>,
    client: reqwest::Client,
}

impl FetchTool {
    pub fn new(policy: Arc<NetworkPolicy>) -> Self {
        Self {
            policy,
            client: reqwest::Client::new(),
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

        // 1. Check Policy
        match self.policy.check(&args.url) {
            Ok(multi_agent_governance::network::NetworkDecision::Allowed) => {}
            Ok(multi_agent_governance::network::NetworkDecision::Denied(reason)) => {
                return Err(Error::tool_execution(format!("Network policy denied access: {}", reason)).into());
            }
            Err(e) => return Err(Error::tool_execution(format!("Policy check failed: {}", e)).into()),
        }

        // 2. Prepare Request
        let method = args.method.to_uppercase().parse::<reqwest::Method>()
            .map_err(|_| Error::tool_execution(format!("Invalid HTTP method: {}", args.method)))?;

        let mut req = self.client.request(method, &args.url);

        for (k, v) in args.headers {
            req = req.header(&k, &v);
        }

        if let Some(body) = args.body {
            req = req.body(body);
        }

        // 3. Execute
        let resp = req.send().await
            .map_err(|e| Error::tool_execution(format!("Request failed: {}", e)))?;

        let status = resp.status();
        let content = resp.text().await
            .map_err(|e| Error::tool_execution(format!("Failed to read response body: {}", e)))?;

        if !status.is_success() {
             return Ok(ToolOutput::text(format!("HTTP Error {}: {}", status, content)));
        }

        Ok(ToolOutput::text(content))
    }
}


/// Tool for downloading files to the sandbox.
#[derive(Clone)]
pub struct DownloadTool {
    policy: Arc<NetworkPolicy>,
    client: reqwest::Client,
    sandbox_manager: Arc<multi_agent_sandbox::SandboxManager>,
}

impl DownloadTool {
    pub fn new(policy: Arc<NetworkPolicy>, sandbox_manager: Arc<multi_agent_sandbox::SandboxManager>) -> Self {
        Self {
            policy,
            client: reqwest::Client::new(),
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

        // 1. Check Policy
        match self.policy.check(&args.url) {
            Ok(multi_agent_governance::network::NetworkDecision::Allowed) => {}
            Ok(multi_agent_governance::network::NetworkDecision::Denied(reason)) => {
                return Err(Error::tool_execution(format!("Network policy denied access: {}", reason)).into());
            }
            Err(e) => return Err(Error::tool_execution(format!("Policy check failed: {}", e)).into()),
        }

        // 2. Download
        let resp = self.client.get(&args.url).send().await
            .map_err(|e| Error::tool_execution(format!("Request failed: {}", e)))?;

        if !resp.status().is_success() {
             return Err(Error::tool_execution(format!("HTTP Error {}", resp.status())).into());
        }

        let content = resp.bytes().await
            .map_err(|e| Error::tool_execution(format!("Failed to download content: {}", e)))?;

        // 3. Write to Sandbox
        let sandbox_id = self.sandbox_manager.get_or_create().await
             .map_err(|e| Error::tool_execution(format!("Failed to get sandbox: {}", e)))?;

        self.sandbox_manager.engine().write_file(&sandbox_id, &args.destination_path, &content).await
            .map_err(|e| Error::tool_execution(format!("Failed to write to sandbox: {}", e)))?;

        Ok(ToolOutput::text(format!("Successfully downloaded {} bytes to {}", content.len(), args.destination_path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multi_agent_governance::network::NetworkPolicy;

    #[tokio::test]
    async fn test_fetch_tool_policy_deny() {
        let policy = Arc::new(NetworkPolicy::default()); // Defaults deny everything
        let tool = FetchTool::new(policy);
        
        let args = serde_json::json!({
            "url": "https://google.com"
        });
        
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Network policy denied access"));
    }

    #[tokio::test]
    async fn test_fetch_tool_policy_allow() {
        let policy = Arc::new(NetworkPolicy::new(
            vec!["google.com".to_string()],
            vec![],
            vec![80, 443]
        ));
        let tool = FetchTool::new(policy);
        
        // We expect a network error (since we have no internet or it might fail), 
        // but NOT a policy error.
        let args = serde_json::json!({
            "url": "https://google.com"
        });
        
        let result = tool.execute(args).await;
        // It might succeed if we have net access, or fail with connection error.
        // We just check it didn't fail with "Network policy denied access".
        if let Err(e) = result {
            assert!(!e.to_string().contains("Network policy denied access"), "Should have passed policy check. Error was: {}", e);
        }
    }
}
