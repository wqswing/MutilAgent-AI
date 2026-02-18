use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteScope {
    Channel,
    Account,
    Peer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoutingContext {
    pub channel: Option<String>,
    pub account: Option<String>,
    pub peer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingDecision {
    pub rule_id: String,
    pub scope: RouteScope,
    pub target: RouteTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

