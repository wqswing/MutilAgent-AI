use serde::{Deserialize, Serialize};
use multi_agent_core::types::ToolRiskLevel;
use std::path::Path;
use anyhow::{Context, Result};

/// A versioned policy document containing security rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyFile {
    pub version: String,
    pub name: String,
    pub rules: Vec<PolicyRule>,
    pub thresholds: PolicyThresholds,
}

/// A single security rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: String,
    pub description: Option<String>,
    pub match_rule: RuleMatch,
    pub action: RuleAction,
}

/// Condition to match a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatch {
    /// Exact tool name to match.
    pub tool: Option<String>,
    /// Glob pattern for tool name (e.g., "sandbox_*").
    pub tool_glob: Option<String>,
    /// List of substrings that must be present in arguments.
    pub args_contain: Option<Vec<String>>,
}

/// Action to take if a rule matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleAction {
    /// Assigned risk level.
    pub risk: ToolRiskLevel,
    /// Human-readable reason for the assignment.
    pub reason: Option<String>,
}

/// Risk score thresholds for different levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyThresholds {
    pub low: u32,
    pub medium: u32,
    pub high: u32,
    pub critical: u32,
    pub approval_required: u32,
}

impl Default for PolicyThresholds {
    fn default() -> Self {
        Self {
            low: 0,
            medium: 25,
            high: 50,
            critical: 75,
            approval_required: 50,
        }
    }
}

/// Merged policy configuration for evaluation.
pub struct PolicyEngine {
    pub policy: PolicyFile,
}

impl PolicyEngine {
    /// Load policy from a YAML file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read policy file: {:?}", path.as_ref()))?;
        let policy: PolicyFile = serde_yaml::from_str(&content)
            .with_context(|| "Failed to parse policy YAML")?;
        Ok(Self { policy })
    }

    /// Create a PolicyEngine from an already loaded PolicyFile.
    /// This is primarily for testing or when the policy is constructed in-memory.
    pub fn from_file(policy: PolicyFile) -> Self {
        Self { policy }
    }

    /// Evaluate a tool call against the loaded policy.
    pub fn evaluate(&self, tool: &str, args: &serde_json::Value) -> PolicyDecision {
        let mut highest_risk = ToolRiskLevel::Low;
        let mut matched_rule_id = None;
        let mut reason = "Default policy (no matching rules)".to_string();

        for rule in &self.policy.rules {
            if self.matches(rule, tool, args) {
                // If this rule has a higher or equal risk than current, update
                if rule.action.risk > highest_risk || matched_rule_id.is_none() {
                    highest_risk = rule.action.risk;
                    matched_rule_id = Some(rule.id.clone());
                    reason = rule.action.reason.clone().unwrap_or_else(|| {
                        format!("Matched rule: {}", rule.id)
                    });
                }
            }
        }

        let risk_score = self.risk_to_score(highest_risk);

        PolicyDecision {
            risk_level: highest_risk,
            risk_score,
            matched_rule: matched_rule_id,
            reason,
            policy_version: self.policy.version.clone(),
        }
    }

    fn matches(&self, rule: &PolicyRule, tool: &str, args: &serde_json::Value) -> bool {
        // 1. Match tool name exactly
        if let Some(tool_name) = &rule.match_rule.tool {
            if tool_name != tool {
                return false;
            }
        }

        // 2. Match tool name glob
        if let Some(glob) = &rule.match_rule.tool_glob {
            if !self.glob_match(glob, tool) {
                return false;
            }
        }

        // 3. Match arguments
        if let Some(substrings) = &rule.match_rule.args_contain {
            let args_str = serde_json::to_string(args).unwrap_or_default().to_lowercase();
            for sub in substrings {
                if !args_str.contains(&sub.to_lowercase()) {
                    return false;
                }
            }
        }

        true
    }

    fn glob_match(&self, pattern: &str, text: &str) -> bool {
        // Simple glob implementation: handle "*" at start, end, or both
        if pattern == "*" {
            return true;
        }
        if pattern.starts_with('*') && pattern.ends_with('*') {
            let inner = &pattern[1..pattern.len() - 1];
            return text.contains(inner);
        }
        if pattern.starts_with('*') {
            let suffix = &pattern[1..];
            return text.ends_with(suffix);
        }
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            return text.starts_with(prefix);
        }
        pattern == text
    }

    fn risk_to_score(&self, risk: ToolRiskLevel) -> u32 {
        match risk {
            ToolRiskLevel::Low => self.policy.thresholds.low,
            ToolRiskLevel::Medium => self.policy.thresholds.medium,
            ToolRiskLevel::High => self.policy.thresholds.high,
            ToolRiskLevel::Critical => self.policy.thresholds.critical,
        }
    }

    /// Merge another policy into this one (other wins on ID conflict).
    pub fn merge(&mut self, other: PolicyFile) {
        // Simple merge: append rules, update thresholds if provided
        for rule in other.rules {
            // Replace existing rule if ID matches
            if let Some(existing) = self.policy.rules.iter_mut().find(|r| r.id == rule.id) {
                *existing = rule;
            } else {
                self.policy.rules.push(rule);
            }
        }
        self.policy.thresholds = other.thresholds;
        self.policy.version = other.version;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_policy() -> PolicyFile {
        PolicyFile {
            version: "1.0".to_string(),
            name: "Test Policy".to_string(),
            rules: vec![
                PolicyRule {
                    id: "block-rm-rf".to_string(),
                    description: None,
                    match_rule: RuleMatch {
                        tool: Some("sandbox_shell".to_string()),
                        tool_glob: None,
                        args_contain: Some(vec!["rm -rf".to_string()]),
                    },
                    action: RuleAction {
                        risk: ToolRiskLevel::Critical,
                        reason: Some("Destructive command detected".to_string()),
                    },
                },
                PolicyRule {
                    id: "read-ops".to_string(),
                    description: None,
                    match_rule: RuleMatch {
                        tool: None,
                        tool_glob: Some("*_read".to_string()),
                        args_contain: None,
                    },
                    action: RuleAction {
                        risk: ToolRiskLevel::Low,
                        reason: Some("Read-only operation".to_string()),
                    },
                },
            ],
            thresholds: PolicyThresholds::default(),
        }
    }

    #[test]
    fn test_exact_match_and_args() {
        let engine = PolicyEngine::from_file(test_policy());
        
        let decision = engine.evaluate("sandbox_shell", &json!({"command": "rm -rf /"}));
        assert_eq!(decision.risk_level, ToolRiskLevel::Critical);
        assert_eq!(decision.matched_rule, Some("block-rm-rf".to_string()));
        
        // Should NOT match if args don't contain rm -rf
        let decision = engine.evaluate("sandbox_shell", &json!({"command": "ls"}));
        assert_eq!(decision.risk_level, ToolRiskLevel::Low);
        assert_eq!(decision.matched_rule, None);
    }

    #[test]
    fn test_glob_match() {
        let engine = PolicyEngine::from_file(test_policy());
        
        let decision = engine.evaluate("fs_read", &json!({}));
        assert_eq!(decision.risk_level, ToolRiskLevel::Low);
        assert_eq!(decision.matched_rule, Some("read-ops".to_string()));
    }

    #[test]
    fn test_merge() {
        let mut engine = PolicyEngine::from_file(test_policy());
        let override_policy = PolicyFile {
            version: "1.1".to_string(),
            name: "Override".to_string(),
            rules: vec![
                PolicyRule {
                    id: "read-ops".to_string(), // Conflict ID
                    description: None,
                    match_rule: RuleMatch {
                        tool: None,
                        tool_glob: Some("*_read".to_string()),
                        args_contain: None,
                    },
                    action: RuleAction {
                        risk: ToolRiskLevel::Medium, // Changed risk
                        reason: Some("Elevated read risk".to_string()),
                    },
                },
            ],
            thresholds: PolicyThresholds {
                medium: 10,
                ..Default::default()
            },
        };

        engine.merge(override_policy);
        
        let decision = engine.evaluate("fs_read", &json!({}));
        assert_eq!(decision.risk_level, ToolRiskLevel::Medium);
        assert_eq!(decision.risk_score, 10); // From overriden threshold
    }
}

/// Result of a policy evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub risk_level: ToolRiskLevel,
    pub risk_score: u32,
    pub matched_rule: Option<String>,
    pub reason: String,
    pub policy_version: String,
}
