use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    #[serde(default)]
    pub channel: RoutingPolicyChannel,
    pub rules: Vec<RoutingRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPolicyChannel {
    Canary,
    #[default]
    Stable,
}

pub struct RoutingPolicyStore {
    active_stable: RwLock<Option<RoutingPolicyRelease>>,
    active_canary: RwLock<Option<RoutingPolicyRelease>>,
    history: RwLock<Vec<RoutingPolicyRelease>>,
    persistence_path: Option<PathBuf>,
}

impl RoutingPolicyStore {
    pub fn new() -> Self {
        Self {
            active_stable: RwLock::new(None),
            active_canary: RwLock::new(None),
            history: RwLock::new(Vec::new()),
            persistence_path: None,
        }
    }

    pub fn new_persistent(path: impl AsRef<Path>) -> multi_agent_core::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut store = Self {
            active_stable: RwLock::new(None),
            active_canary: RwLock::new(None),
            history: RwLock::new(Vec::new()),
            persistence_path: Some(path.clone()),
        };
        if path.exists() {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                multi_agent_core::Error::invalid_request(format!(
                    "Read routing policy store failed: {}",
                    e
                ))
            })?;
            if !content.trim().is_empty() {
                let snapshot: RoutingPolicySnapshot =
                    serde_json::from_str(&content).map_err(|e| {
                        multi_agent_core::Error::invalid_request(format!(
                            "Parse routing policy store failed: {}",
                            e
                        ))
                    })?;
                store.active_stable = RwLock::new(snapshot.active_stable);
                store.active_canary = RwLock::new(snapshot.active_canary);
                store.history = RwLock::new(snapshot.history);
            }
        }
        Ok(store)
    }

    async fn persist_snapshot(&self) -> multi_agent_core::Result<()> {
        let Some(path) = &self.persistence_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                multi_agent_core::Error::invalid_request(format!(
                    "Create routing policy dir failed: {}",
                    e
                ))
            })?;
        }
        let snapshot = RoutingPolicySnapshot {
            active_stable: self.active_stable.read().await.clone(),
            active_canary: self.active_canary.read().await.clone(),
            history: self.history.read().await.clone(),
        };
        let content = serde_json::to_string_pretty(&snapshot).map_err(|e| {
            multi_agent_core::Error::invalid_request(format!(
                "Serialize routing policy snapshot failed: {}",
                e
            ))
        })?;
        std::fs::write(path, content).map_err(|e| {
            multi_agent_core::Error::invalid_request(format!(
                "Persist routing policy store failed: {}",
                e
            ))
        })?;
        Ok(())
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
        match release.channel {
            RoutingPolicyChannel::Stable => {
                let mut active = self.active_stable.write().await;
                *active = Some(release);
            }
            RoutingPolicyChannel::Canary => {
                let mut active = self.active_canary.write().await;
                *active = Some(release);
            }
        }
        self.persist_snapshot().await?;
        Ok(())
    }

    pub async fn rollback_to(&self, version: &str) -> multi_agent_core::Result<()> {
        let history = self.history.read().await;
        let Some(found) = history.iter().find(|r| r.version == version).cloned() else {
            return Err(multi_agent_core::Error::invalid_request(format!(
                "Policy version '{}' not found",
                version
            )));
        };
        drop(history);
        match found.channel {
            RoutingPolicyChannel::Stable => {
                let mut active = self.active_stable.write().await;
                *active = Some(found);
            }
            RoutingPolicyChannel::Canary => {
                let mut active = self.active_canary.write().await;
                *active = Some(found);
            }
        }
        self.persist_snapshot().await?;
        Ok(())
    }

    pub async fn promote_canary_to_stable(
        &self,
        version: Option<&str>,
    ) -> multi_agent_core::Result<()> {
        let canary = self.active_canary.read().await.clone();
        let Some(mut release) = canary else {
            return Err(multi_agent_core::Error::invalid_request(
                "No active canary routing policy".to_string(),
            ));
        };
        if let Some(v) = version {
            if release.version != v {
                return Err(multi_agent_core::Error::invalid_request(format!(
                    "Active canary version '{}' does not match requested '{}'",
                    release.version, v
                )));
            }
        }
        release.channel = RoutingPolicyChannel::Stable;
        let mut stable = self.active_stable.write().await;
        *stable = Some(release);
        drop(stable);
        self.persist_snapshot().await?;
        Ok(())
    }

    pub async fn active_release(&self) -> Option<RoutingPolicyRelease> {
        if let Some(stable) = self.active_stable.read().await.clone() {
            return Some(stable);
        }
        self.active_canary.read().await.clone()
    }

    pub async fn active_release_for_channel(
        &self,
        channel: RoutingPolicyChannel,
    ) -> Option<RoutingPolicyRelease> {
        match channel {
            RoutingPolicyChannel::Stable => self.active_stable.read().await.clone(),
            RoutingPolicyChannel::Canary => self.active_canary.read().await.clone(),
        }
    }

    pub async fn active_channels(&self) -> (Option<RoutingPolicyRelease>, Option<RoutingPolicyRelease>) {
        (
            self.active_stable.read().await.clone(),
            self.active_canary.read().await.clone(),
        )
    }

    pub async fn list_versions(&self) -> Vec<RoutingPolicyRelease> {
        self.history.read().await.clone()
    }

    pub async fn resolve(&self, context: &RoutingContext) -> Option<RoutingDecision> {
        let active = self.active_release().await?;
        let engine = RoutingPolicyEngine::new(active.rules);
        engine.resolve(context)
    }

    pub async fn resolve_for_channel(
        &self,
        context: &RoutingContext,
        channel: RoutingPolicyChannel,
    ) -> Option<RoutingDecision> {
        let active = self.active_release_for_channel(channel).await?;
        let engine = RoutingPolicyEngine::new(active.rules);
        engine.resolve(context)
    }

    pub async fn simulate_active(
        &self,
        scenarios: &[RoutingContext],
    ) -> Option<Vec<RoutingSimulation>> {
        let active = self.active_release().await?;
        let engine = RoutingPolicyEngine::new(active.rules);
        Some(engine.simulate(scenarios))
    }
}

pub type SharedRoutingPolicyStore = Arc<RoutingPolicyStore>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RoutingPolicySnapshot {
    active_stable: Option<RoutingPolicyRelease>,
    active_canary: Option<RoutingPolicyRelease>,
    history: Vec<RoutingPolicyRelease>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_persistent_store_publish_and_reload() {
        let path = std::env::temp_dir().join(format!(
            "routing_policy_{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = RoutingPolicyStore::new_persistent(&path).expect("create store");
        store
            .publish(RoutingPolicyRelease {
                version: "1.0.0".to_string(),
                name: Some("baseline".to_string()),
                published_at: 0,
                channel: RoutingPolicyChannel::Stable,
                rules: vec![RoutingRule::force_fast(
                    "r1",
                    RouteScope::Channel,
                    "c1",
                    "search",
                    1,
                )],
            })
            .await
            .expect("publish");

        let reloaded = RoutingPolicyStore::new_persistent(&path).expect("reload store");
        let active = reloaded.active_release().await.expect("active release");
        assert_eq!(active.version, "1.0.0");
        assert_eq!(reloaded.list_versions().await.len(), 1);
    }

    #[tokio::test]
    async fn test_store_rollback_to_previous_version() {
        let store = RoutingPolicyStore::new();
        store
            .publish(RoutingPolicyRelease {
                version: "1.0.0".to_string(),
                name: Some("v1".to_string()),
                published_at: 0,
                channel: RoutingPolicyChannel::Stable,
                rules: vec![RoutingRule::force_fast(
                    "r1",
                    RouteScope::Account,
                    "a1",
                    "search",
                    1,
                )],
            })
            .await
            .expect("publish v1");
        store
            .publish(RoutingPolicyRelease {
                version: "1.1.0".to_string(),
                name: Some("v2".to_string()),
                published_at: 0,
                channel: RoutingPolicyChannel::Stable,
                rules: vec![RoutingRule::force_fast(
                    "r2",
                    RouteScope::Channel,
                    "c1",
                    "calculator",
                    1,
                )],
            })
            .await
            .expect("publish v2");

        store.rollback_to("1.0.0").await.expect("rollback");
        let active = store.active_release().await.expect("active after rollback");
        assert_eq!(active.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_promote_canary_to_stable() {
        let store = RoutingPolicyStore::new();
        store
            .publish(RoutingPolicyRelease {
                version: "1.2.0".to_string(),
                name: Some("canary".to_string()),
                published_at: 0,
                channel: RoutingPolicyChannel::Canary,
                rules: vec![RoutingRule::force_fast(
                    "r3",
                    RouteScope::Channel,
                    "c-canary",
                    "search",
                    1,
                )],
            })
            .await
            .expect("publish canary");

        store
            .promote_canary_to_stable(Some("1.2.0"))
            .await
            .expect("promote");
        let stable = store
            .active_release_for_channel(RoutingPolicyChannel::Stable)
            .await
            .expect("stable");
        assert_eq!(stable.version, "1.2.0");
    }
}
