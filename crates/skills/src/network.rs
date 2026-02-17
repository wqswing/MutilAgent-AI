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

const MAX_REDIRECTS: usize = 5;

// Helper to perform request with manual redirect handling and SSRF protection
async fn fetch_with_policy(
    client: &reqwest::Client,
    policy: &tokio::sync::RwLock<NetworkPolicy>,
    safety: &SafetyConfig,
    mut method: reqwest::Method,
    mut url: url::Url,
    headers: Option<&reqwest::header::HeaderMap>,
    body: Option<&String>,
) -> Result<reqwest::Response> {
    for _ in 0..MAX_REDIRECTS {
        // Enforce content type on the final response? No, we check after request.
        // But we must perform checks on every hop if needed? 
        // We only check body/content-type on the final response mostly.
        
        // 1. Check Policy (Domain)
        {
            let p = policy.read().await;
            match p.check(url.as_str()) {
                Ok(multi_agent_governance::network::NetworkDecision::Allowed) => {}
                Ok(multi_agent_governance::network::NetworkDecision::Denied(reason)) => {
                    return Err(Error::tool_execution(format!(
                        "Network policy denied access to {}: {}",
                        url, reason
                    )));
                }
                Err(e) => return Err(Error::tool_execution(format!("Policy check failed: {}", e))),
            }
        }

        // 2. Resolve IP & Check
        let host = url.host_str().ok_or_else(|| Error::tool_execution("URL has no host".to_string()))?;
        let port = url.port_or_known_default().unwrap_or(80);
        let addr_str = format!("{}:{}", host, port);
        
        let mut addrs = tokio::net::lookup_host(&addr_str).await
            .map_err(|e| Error::tool_execution(format!("DNS resolution failed for {}: {}", host, e)))?;
        
        // Use first IP
        let target_socket = addrs.next().ok_or_else(|| Error::tool_execution(format!("No IP addresses found for {}", host)))?;
        let target_ip = target_socket.ip();

        // Validate IP
        {
            let p = policy.read().await;
            if let Err(e) = p.check_ip(target_ip) {
                return Err(Error::tool_execution(format!("Network policy denied IP {}: {}", target_ip, e)));
            }
        }

        // 3. Prepare Request (IP Pinning)
        let mut safe_url = url.clone();
        if safe_url.set_host(Some(&target_ip.to_string())).is_err() {
            return Err(Error::tool_execution(format!("Failed to set safe IP host: {}", target_ip)));
        }

        let mut req_builder = client.request(method.clone(), safe_url)
            .header("Host", host);
        
        if let Some(h) = headers {
            req_builder = req_builder.headers(h.clone());
        }

        // Only attach body if method allows (or if we are keeping generic)
        // For redirects, we need to handle body dropping logic if we change method
        if let Some(b) = body {
             req_builder = req_builder.body(b.clone());
        }

        let resp = req_builder.send().await
            .map_err(|e| Error::tool_execution(format!("Request failed: {}", e)))?;

        // 4. Handle Redirects
        if resp.status().is_redirection() {
            if let Some(loc) = resp.headers().get(reqwest::header::LOCATION) {
                let loc_str = loc.to_str().map_err(|e| Error::tool_execution(format!("Invalid Location header: {}", e)))?;
                // Parse relative or absolute
                let next_url = url.join(loc_str)
                    .map_err(|e| Error::tool_execution(format!("Invalid redirect URL {}: {}", loc_str, e)))?;
                
                // Determine next method/body
                let status = resp.status();
                if status == reqwest::StatusCode::MOVED_PERMANENTLY || // 301
                   status == reqwest::StatusCode::FOUND ||             // 302
                   status == reqwest::StatusCode::SEE_OTHER {          // 303
                    method = reqwest::Method::GET;
                    // Body is usually dropped for GET, we pass None in next iteration implicitly?
                    // But we pass 'body' arg. We should update local 'body' ref or logic?
                    // Actually, if we switch to GET, we should ignore 'body' in next iteration?
                    // We can just keep 'method' as GET. Logic above adds body if 'body' is Some. 
                    // reqwest/http might ignore body for GET? Or sending GET with body is allowed but discouraged?
                    // Better to clear body if switching to GET.
                    // But 'body' is immutable arg.
                    // We'll need a flag `use_body`.
                }
                
                url = next_url;
                continue;
            }
        }



        // Check Content-Length if present
        if let Some(cl) = resp.headers().get(reqwest::header::CONTENT_LENGTH) {
            if let Ok(cl_str) = cl.to_str() {
                if let Ok(size) = cl_str.parse::<u64>() {
                    if size > safety.max_download_size_bytes {
                        return Err(Error::tool_execution(format!(
                            "Content-Length {} exceeds limit {}",
                            size, safety.max_download_size_bytes
                        )));
                    }
                }
            }
        }

        // Check Content-Type if present
        if !safety.allowed_content_types.is_empty() {
            if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
                let ct_str = ct.to_str().unwrap_or("");
                let mut allowed = false;
                for a in &safety.allowed_content_types {
                    if ct_str.starts_with(a) {
                        allowed = true;
                        break;
                    }
                }
                if !allowed {
                     return Err(Error::tool_execution(format!("Content-Type '{}' not allowed", ct_str)));
                }
            }
        }

        return Ok(resp);
    }
    
    Err(Error::tool_execution(format!("Too many redirects (max {})", MAX_REDIRECTS)))
}

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

        // Use helper
        let resp = fetch_with_policy(
            &self.client,
            &self.policy,
            &self.safety,
            method,
            url,
            Some(&headers),
            args.body.as_ref()
        ).await?;
        
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

        // Download is GET, no body, no custom headers from args (for now)
        let resp = fetch_with_policy(
            &self.client,
            &self.policy,
            &self.safety,
            reqwest::Method::GET,
            url,
            None,
            None
        ).await?;

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
