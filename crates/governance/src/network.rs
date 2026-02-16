//! Network policy governance.
//!
//! This module defines rules for controlling outbound network access from the agent.
//! By default, all network access is denied unless explicitly allowed.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Network policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// List of allowed domains (supports wildcards like `*.google.com`).
    pub allow_domains: Vec<String>,
    /// List of explicitly denied domains (takes precedence over allow).
    pub deny_domains: Vec<String>,
    /// List of allowed destination ports.
    pub allow_ports: Vec<u16>,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow_domains: vec![],
            deny_domains: vec![],
            allow_ports: vec![80, 443], // HTTP/HTTPS allowed by default if domain matches
        }
    }
}

/// Network access decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkDecision {
    /// Access allowed.
    Allowed,
    /// Access denied with reason.
    Denied(String),
}

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

impl NetworkPolicy {
    /// Create a new network policy.
    pub fn new(allow_domains: Vec<String>, deny_domains: Vec<String>, allow_ports: Vec<u16>) -> Self {
        Self {
            allow_domains,
            deny_domains,
            allow_ports,
        }
    }

    /// Check if a URL is allowed by the policy.
    ///
    /// Rules:
    /// 1. IP addresses are currently DENIED (enforce DNS usage).
    /// 2. Port must be in `allow_ports`.
    /// 3. Domain must NOT be in `deny_domains`.
    /// 4. Domain MUST be in `allow_domains`.
    pub fn check(&self, url_str: &str) -> Result<NetworkDecision, NetworkError> {
        let url = Url::parse(url_str).map_err(|e| NetworkError::InvalidUrl(e.to_string()))?;

        // 1. Check Port
        let port = url.port_or_known_default().unwrap_or(80);
        if !self.allow_ports.contains(&port) {
            return Ok(NetworkDecision::Denied(format!("Port {} is not allowed", port)));
        }

        // 2. Check Host
        let host = match url.host_str() {
            Some(h) => h,
            None => return Ok(NetworkDecision::Denied("URL has no host".to_string())),
        };

        // Block IP addresses (simple check: if it parses as IP, block it)
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(NetworkDecision::Denied("Direct IP access is prohibited. Use domain names.".to_string()));
        }

        // 3. Check Deny List
        for rule in &self.deny_domains {
            if self.matches(host, rule) {
                return Ok(NetworkDecision::Denied(format!("Domain '{}' is explicitly denied by rule '{}'", host, rule)));
            }
        }

        // 4. Check Allow List
        for rule in &self.allow_domains {
            if self.matches(host, rule) {
                return Ok(NetworkDecision::Allowed);
            }
        }

        Ok(NetworkDecision::Denied(format!("Domain '{}' is not in the allowlist", host)))
    }

    /// Helper to match domains with wildcards.
    /// Supported usage: `example.com`, `*.example.com`, `*`.
    fn matches(&self, domain: &str, rule: &str) -> bool {
        if rule == "*" {
            return true;
        }

        if rule.starts_with("*.") {
            let suffix = &rule[2..];
            return domain.ends_with(suffix) || domain == suffix;
        }

        domain == rule
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_deny() {
        let policy = NetworkPolicy::default();
        let result = policy.check("https://google.com").unwrap();
        assert!(matches!(result, NetworkDecision::Denied(_)));
    }

    #[test]
    fn test_allow_domain() {
        let policy = NetworkPolicy::new(
            vec!["google.com".to_string()],
            vec![],
            vec![443],
        );
        let result = policy.check("https://google.com").unwrap();
        assert_eq!(result, NetworkDecision::Allowed);
    }

    #[test]
    fn test_wildcard_allow() {
        let policy = NetworkPolicy::new(
            vec!["*.google.com".to_string()],
            vec![],
            vec![443],
        );
        assert_eq!(policy.check("https://mail.google.com").unwrap(), NetworkDecision::Allowed);
        assert_eq!(policy.check("https://google.com").unwrap(), NetworkDecision::Allowed);
        assert!(matches!(policy.check("https://yahoo.com").unwrap(), NetworkDecision::Denied(_)));
    }

    #[test]
    fn test_explicit_deny_precedence() {
        let policy = NetworkPolicy::new(
            vec!["*.google.com".to_string()],
            vec!["mail.google.com".to_string()],
            vec![443],
        );
        // Explicitly denied
        let result = policy.check("https://mail.google.com").unwrap();
        assert!(matches!(result, NetworkDecision::Denied(reason) if reason.contains("explicitly denied")));

        // Allowed by wildcard
        assert_eq!(policy.check("https://maps.google.com").unwrap(), NetworkDecision::Allowed);
    }

    #[test]
    fn test_port_restriction() {
        let policy = NetworkPolicy::new(
            vec!["google.com".to_string()],
            vec![],
            vec![443],
        );
        // Port 80 not allowed
        let result = policy.check("http://google.com").unwrap(); // http implies 80
        assert!(matches!(result, NetworkDecision::Denied(reason) if reason.contains("Port 80")));
    }

    #[test]
    fn test_ip_block() {
        let policy = NetworkPolicy::new(
            vec!["*".to_string()],
            vec![],
            vec![443],
        );
        let result = policy.check("https://1.1.1.1").unwrap();
        assert!(matches!(result, NetworkDecision::Denied(reason) if reason.contains("Direct IP access")));
    }
}
