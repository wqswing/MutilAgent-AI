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
    /// Policy version (UUID).
    pub version: String,
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
            version: uuid::Uuid::new_v4().to_string(),
            allow_domains: vec![],
            deny_domains: vec![],
            allow_ports: vec![80, 443], // HTTP/HTTPS allowed by default if domain matches
        }
    }
}

/// Network access decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fn new(
        allow_domains: Vec<String>,
        deny_domains: Vec<String>,
        allow_ports: Vec<u16>,
    ) -> Self {
        Self {
            version: uuid::Uuid::new_v4().to_string(),
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
    /// Check if a URL is allowed by the policy.
    ///
    /// Rules:
    /// 1. Port must be in `allow_ports`.
    /// 2. Domain must NOT be in `deny_domains`.
    /// 3. Domain MUST be in `allow_domains`.
    /// 4. If Host is an IP, it must be public (not loopback/private).
    pub fn check(&self, url_str: &str) -> Result<NetworkDecision, NetworkError> {
        let url = Url::parse(url_str).map_err(|e| NetworkError::InvalidUrl(e.to_string()))?;

        // 1. Check Port
        let port = url.port_or_known_default().unwrap_or(80);
        if !self.allow_ports.contains(&port) {
            return Ok(NetworkDecision::Denied(format!(
                "Port {} is not allowed",
                port
            )));
        }

        // 2. Check Host
        let host = match url.host_str() {
            Some(h) => h,
            None => return Ok(NetworkDecision::Denied("URL has no host".to_string())),
        };

        // If it parses as IP, check it immediately using check_ip
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
             match self.check_ip(ip) {
                 Ok(_) => {}, // IP is safe, but we still need to check allow/deny lists if we treat IPs as domains? 
                              // Actually, if it's an IP, we probably just want to check if it's private.
                              // But wait, the previous logic said "Direct IP access is prohibited". 
                              // Use domain names.
                              // Let's stick to "Direct IP access is prohibited" for now as per M6.1 plan?
                              // "Implement logic to block... Private...". 
                              // Actually, the plan says "Add check_ip method... Implement logic to block...".
                              // If I allow public IPs, I should check them.
                              // But the previous code explicitly blocked ALL IPs. 
                              // "Direct IP access is prohibited. Use domain names."
                              // Use domain names implies we DO NOT want users hitting 1.1.1.1 directly?
                              // If so, `check` handles the URL string check.
                              // `check_ip` will be used by the caller (FetchTool) AFTER DNS resolution.
                 Err(e) => return Ok(NetworkDecision::Denied(e.to_string())),
             }
             // Fall through to deny/allow lists? IP usually won't match "google.com".
             // If allow list has "*", it might match.
        }
        
        // Re-implement existing logic but maybe lift the "Block IP addresses" restriction if we want to allow public IPs?
        // The original code:
        /*
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(NetworkDecision::Denied(
                "Direct IP access is prohibited. Use domain names.".to_string(),
            ));
        }
        */
        // I will Keep this restriction for `check(url)` because we want to enforce DNS usage for visibility/policy.
        // `check_ip` is a helper for the Tool to use AFTER resolving the domain.
        
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(NetworkDecision::Denied(
                "Direct IP access is prohibited. Use domain names.".to_string(),
            ));
        }

        // 3. Check Deny List
        for rule in &self.deny_domains {
            if self.matches(host, rule) {
                return Ok(NetworkDecision::Denied(format!(
                    "Domain '{}' is explicitly denied by rule '{}'",
                    host, rule
                )));
            }
        }

        // 4. Check Allow List
        for rule in &self.allow_domains {
            if self.matches(host, rule) {
                return Ok(NetworkDecision::Allowed);
            }
        }

        Ok(NetworkDecision::Denied(format!(
            "Domain '{}' is not in the allowlist",
            host
        )))
    }

    /// Check if an IP address is allowed (must be public).
    pub fn check_ip(&self, ip: std::net::IpAddr) -> Result<(), String> {
        match ip {
            std::net::IpAddr::V4(ipv4) => Self::check_ipv4(ipv4),
            std::net::IpAddr::V6(ipv6) => {
                // Handle IPv4-mapped IPv6 addresses (::ffff:0:0/96)
                if let Some(ipv4) = ipv6.to_ipv4() {
                    return Self::check_ipv4(ipv4);
                }

                if ipv6.is_loopback() 
                    || ipv6.is_unspecified() 
                    // unique local: fc00::/7
                    || (ipv6.segments()[0] & 0xfe00) == 0xfc00 
                    // link local: fe80::/10
                    || (ipv6.segments()[0] & 0xffc0) == 0xfe80
                    // discarded: 100::/64
                    || (ipv6.segments()[0] == 0x0100 && ipv6.segments()[1] == 0 && ipv6.segments()[2] == 0 && ipv6.segments()[3] == 0)
                    // documentation: 2001:db8::/32
                    || (ipv6.segments()[0] == 0x2001 && ipv6.segments()[1] == 0x0db8)
                {
                    return Err(format!("Blocked internal/private IPv6: {}", ipv6));
                }
                Ok(())
            }
        }
    }

    fn check_ipv4(ipv4: std::net::Ipv4Addr) -> Result<(), String> {
        if ipv4.is_loopback() 
            || ipv4.is_private() 
            || ipv4.is_link_local() 
            || ipv4.is_broadcast() 
            || ipv4.is_documentation() 
            || ipv4.is_unspecified()
        {
            return Err(format!("Blocked internal/private IPv4: {}", ipv4));
        }

        let octets = ipv4.octets();
        
        // Carrier-grade NAT (100.64.0.0/10)
        // 100.64.0.0 to 100.127.255.255
        if octets[0] == 100 && (octets[1] & 0xC0) == 0x40 {
             return Err(format!("Blocked Carrier-Grade NAT IPv4: {}", ipv4));
        }

        // IETF Protocol Assignments (192.0.0.0/24)
        if octets[0] == 192 && octets[1] == 0 && octets[2] == 0 {
             return Err(format!("Blocked IETF Protocol Assignment IPv4: {}", ipv4));
        }

        // Benchmarking (198.18.0.0/15)
        // 198.18.0.0 to 198.19.255.255
        if octets[0] == 198 && (octets[1] & 0xFE) == 18 {
             return Err(format!("Blocked Benchmarking IPv4: {}", ipv4));
        }

        // Reserved (240.0.0.0/4) - Class E (except broadcast 255.255.255.255 which is covered)
        // 240.0.0.0 to 255.255.255.254
        if octets[0] >= 240 {
             // 255.255.255.255 is broadcast, already checked. 
             // But let's block the rest of Class E just in case.
             if ipv4 != std::net::Ipv4Addr::BROADCAST {
                 return Err(format!("Blocked Reserved/Class E IPv4: {}", ipv4));
             }
        }
        
        Ok(())
    }

    /// Helper to match domains with wildcards.
    /// Supported usage: `example.com`, `*.example.com`, `*`.
    fn matches(&self, domain: &str, rule: &str) -> bool {
        if rule == "*" {
            return true;
        }

        if let Some(suffix) = rule.strip_prefix("*.") {
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
        let policy = NetworkPolicy::new(vec!["google.com".to_string()], vec![], vec![443]);
        let result = policy.check("https://google.com").unwrap();
        assert_eq!(result, NetworkDecision::Allowed);
    }

    #[test]
    fn test_wildcard_allow() {
        let policy = NetworkPolicy::new(vec!["*.google.com".to_string()], vec![], vec![443]);
        assert_eq!(
            policy.check("https://mail.google.com").unwrap(),
            NetworkDecision::Allowed
        );
        assert_eq!(
            policy.check("https://google.com").unwrap(),
            NetworkDecision::Allowed
        );
        assert!(matches!(
            policy.check("https://yahoo.com").unwrap(),
            NetworkDecision::Denied(_)
        ));
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
        assert!(
            matches!(result, NetworkDecision::Denied(reason) if reason.contains("explicitly denied"))
        );

        // Allowed by wildcard
        assert_eq!(
            policy.check("https://maps.google.com").unwrap(),
            NetworkDecision::Allowed
        );
    }

    #[test]
    fn test_port_restriction() {
        let policy = NetworkPolicy::new(vec!["google.com".to_string()], vec![], vec![443]);
        // Port 80 not allowed
        let result = policy.check("http://google.com").unwrap(); // http implies 80
        assert!(matches!(result, NetworkDecision::Denied(reason) if reason.contains("Port 80")));
    }

    #[test]
    fn test_ip_block() {
        let policy = NetworkPolicy::new(vec!["*".to_string()], vec![], vec![443]);
        let result = policy.check("https://1.1.1.1").unwrap();
        assert!(
            matches!(result, NetworkDecision::Denied(reason) if reason.contains("Direct IP access"))
        );
    }

    #[test]
    fn test_ssrf_blocks() {
        let policy = NetworkPolicy::default();
        
        // IPv4-mapped IPv6 Loopback
        let ip: std::net::IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert!(policy.check_ip(ip).is_err(), "Should block IPv4-mapped loopback");

        // IPv4-mapped IPv6 Private
        let ip: std::net::IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert!(policy.check_ip(ip).is_err(), "Should block IPv4-mapped private");

        // Carrier-Grade NAT
        let ip: std::net::IpAddr = "100.64.0.1".parse().unwrap();
        assert!(policy.check_ip(ip).is_err(), "Should block CGNAT");

        // Cloud Metadata
        let ip: std::net::IpAddr = "169.254.169.254".parse().unwrap();
        assert!(policy.check_ip(ip).is_err(), "Should block Metadata");

        // Benchmarking
        let ip: std::net::IpAddr = "198.18.0.1".parse().unwrap();
        assert!(policy.check_ip(ip).is_err(), "Should block Benchmarking");

        // Class E (Reserved)
        let ip: std::net::IpAddr = "240.0.0.1".parse().unwrap();
        assert!(policy.check_ip(ip).is_err(), "Should block Class E");

        // IPv6 Unique Local
        let ip: std::net::IpAddr = "fc00::1".parse().unwrap();
        assert!(policy.check_ip(ip).is_err(), "Should block IPv6 Unique Local");

        // Public IP (Cloudflare DNS) - Should Pass
        let ip: std::net::IpAddr = "1.1.1.1".parse().unwrap();
        assert!(policy.check_ip(ip).is_ok(), "Should allow public IP");
    }
}
