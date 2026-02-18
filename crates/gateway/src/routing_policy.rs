use semver::Version;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteScope {
    Channel,
    Account,
    Peer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RouteTarget {
    FastAction { tool_name: String },
    ComplexMission { goal_hint: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub scope: RouteScope,
    pub scope_value: String,
    pub target: RouteTarget,
    pub priority: u32,
}

impl RoutingRule {
    pub fn force_fast(
        id: impl Into<String>,
        scope: RouteScope,
        scope_value: impl Into<String>,
        tool_name: impl Into<String>,
        priority: u32,
    ) -> Self {
        Self {
            id: id.into(),
            scope,
            scope_value: scope_value.into(),
            target: RouteTarget::FastAction {
                tool_name: tool_name.into(),
            },
            priority,
        }
    }

    pub fn force_complex(
        id: impl Into<String>,
        scope: RouteScope,
        scope_value: impl Into<String>,
        goal_hint: impl Into<String>,
        priority: u32,
    ) -> Self {
        Self {
            id: id.into(),
            scope,
            scope_value: scope_value.into(),
            target: RouteTarget::ComplexMission {
                goal_hint: goal_hint.into(),
            },
            priority,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingContext {
    pub channel: Option<String>,
    pub account: Option<String>,
    pub peer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub rule_id: String,
    pub scope: RouteScope,
    pub target: RouteTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingSimulation {
    pub matched_rule_id: Option<String>,
    pub scope: Option<RouteScope>,
}

pub struct RoutingPolicyEngine {
    rules: Vec<RoutingRule>,
    precedence: Vec<RouteScope>,
}

impl RoutingPolicyEngine {
    pub fn new(rules: Vec<RoutingRule>) -> Self {
        Self {
            rules,
            precedence: vec![RouteScope::Channel, RouteScope::Account, RouteScope::Peer],
        }
    }

    fn scope_rank(&self, scope: RouteScope) -> usize {
        self.precedence
            .iter()
            .position(|s| *s == scope)
            .unwrap_or(usize::MAX)
    }

    fn context_value<'a>(&self, context: &'a RoutingContext, scope: RouteScope) -> Option<&'a str> {
        match scope {
            RouteScope::Channel => context.channel.as_deref(),
            RouteScope::Account => context.account.as_deref(),
            RouteScope::Peer => context.peer.as_deref(),
        }
    }

    pub fn resolve(&self, context: &RoutingContext) -> Option<RoutingDecision> {
        let mut matches: Vec<&RoutingRule> = self
            .rules
            .iter()
            .filter(|rule| {
                self.context_value(context, rule.scope)
                    .is_some_and(|v| v == rule.scope_value)
            })
            .collect();

        matches.sort_by(|a, b| {
            self.scope_rank(a.scope)
                .cmp(&self.scope_rank(b.scope))
                .then_with(|| b.priority.cmp(&a.priority))
                .then_with(|| a.id.cmp(&b.id))
        });

        matches.first().map(|rule| RoutingDecision {
            rule_id: rule.id.clone(),
            scope: rule.scope,
            target: rule.target.clone(),
        })
    }

    pub fn simulate(&self, scenarios: &[RoutingContext]) -> Vec<RoutingSimulation> {
        scenarios
            .iter()
            .map(|ctx| {
                if let Some(decision) = self.resolve(ctx) {
                    RoutingSimulation {
                        matched_rule_id: Some(decision.rule_id),
                        scope: Some(decision.scope),
                    }
                } else {
                    RoutingSimulation {
                        matched_rule_id: None,
                        scope: None,
                    }
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPolicyRelease {
    pub version: String,
    pub name: Option<String>,
    pub published_at: i64,
    pub rules: Vec<RoutingRule>,
}

#[derive(Default)]
pub struct RoutingPolicyStore {
    active: RwLock<Option<RoutingPolicyRelease>>,
    history: RwLock<Vec<RoutingPolicyRelease>>,
}

impl RoutingPolicyStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn publish(&self, mut release: RoutingPolicyRelease) -> multi_agent_core::Result<()> {
        let version = Version::parse(&release.version).map_err(|e| {
            multi_agent_core::Error::invalid_request(format!("Invalid policy version: {}", e))
        })?;
        if release.published_at <= 0 {
            release.published_at = chrono::Utc::now().timestamp();
        }

        let mut history = self.history.write().await;
        if history.iter().any(|p| p.version == release.version) {
            return Err(multi_agent_core::Error::invalid_request(format!(
                "Policy version '{}' already exists",
                release.version
            )));
        }
        if let Some(last) = history.last() {
            let last_version = Version::parse(&last.version).map_err(|e| {
                multi_agent_core::Error::invalid_request(format!(
                    "Stored policy version parse failed: {}",
                    e
                ))
            })?;
            if version <= last_version {
                return Err(multi_agent_core::Error::invalid_request(format!(
                    "Policy version '{}' must be greater than current '{}'",
                    release.version, last.version
                )));
            }
        }

        history.push(release.clone());
        drop(history);
        let mut active = self.active.write().await;
        *active = Some(release);
        Ok(())
    }

    pub async fn active_release(&self) -> Option<RoutingPolicyRelease> {
        self.active.read().await.clone()
    }

    pub async fn list_versions(&self) -> Vec<RoutingPolicyRelease> {
        self.history.read().await.clone()
    }

    pub async fn resolve(&self, context: &RoutingContext) -> Option<RoutingDecision> {
        let active = self.active.read().await.clone()?;
        let engine = RoutingPolicyEngine::new(active.rules);
        engine.resolve(context)
    }

    pub async fn simulate_active(
        &self,
        scenarios: &[RoutingContext],
    ) -> Option<Vec<RoutingSimulation>> {
        let active = self.active.read().await.clone()?;
        let engine = RoutingPolicyEngine::new(active.rules);
        Some(engine.simulate(scenarios))
    }
}

pub type SharedRoutingPolicyStore = Arc<RoutingPolicyStore>;
