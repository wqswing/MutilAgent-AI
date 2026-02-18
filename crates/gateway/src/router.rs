//! Intent router for classifying incoming requests.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::routing_policy::{
    RouteScope, RouteTarget, RoutingContext, RoutingPolicyChannel, RoutingPolicyEngine,
    SharedRoutingPolicyStore,
};
use multi_agent_core::{
    traits::{ChatMessage, IntentRouter, LlmClient, ToolRegistry},
    types::{NormalizedRequest, UserIntent},
    Result,
};

/// Keywords that suggest a fast action (direct tool call).
const FAST_ACTION_KEYWORDS: &[&str] = &[
    "search",
    "find",
    "lookup",
    "get",
    "fetch",
    "read",
    "list",
    "show",
    "what is",
    "who is",
    "calculate",
    "convert",
];

/// Keywords that suggest a complex mission (requires planning).
const COMPLEX_MISSION_KEYWORDS: &[&str] = &[
    "create",
    "build",
    "develop",
    "implement",
    "design",
    "analyze",
    "review",
    "fix",
    "debug",
    "refactor",
    "optimize",
    "explain",
    "compare",
    "plan",
    "help me",
    "how do i",
    "write",
    "generate",
];

#[derive(Debug, Deserialize)]
struct LlmIntentDecision {
    intent_type: String,
    tool_name: Option<String>,
    args: Option<serde_json::Value>,
    goal: Option<String>,
    confidence: Option<f32>,
}

/// Default router implementation.
///
/// If configured with an LLM + ToolRegistry, it performs capability-driven intent
/// classification first. Keyword routing remains as deterministic fallback.
pub struct DefaultRouter {
    /// Custom fast action patterns.
    fast_patterns: Vec<String>,
    /// Custom complex mission patterns.
    complex_patterns: Vec<String>,
    /// Optional LLM classifier.
    llm_client: Option<Arc<dyn LlmClient>>,
    /// Optional runtime tool capability source.
    tool_registry: Option<Arc<dyn ToolRegistry>>,
    /// Minimum confidence required to trust LLM classification.
    llm_min_confidence: f32,
    /// Optional explicit routing policy.
    routing_policy: Option<Arc<RoutingPolicyEngine>>,
    /// Optional versioned policy store.
    routing_policy_store: Option<SharedRoutingPolicyStore>,
}

impl DefaultRouter {
    /// Create a new default router.
    pub fn new() -> Self {
        Self {
            fast_patterns: Vec::new(),
            complex_patterns: Vec::new(),
            llm_client: None,
            tool_registry: None,
            llm_min_confidence: 0.6,
            routing_policy: None,
            routing_policy_store: None,
        }
    }

    /// Enable LLM-based intent classification.
    pub fn with_llm_classifier(
        mut self,
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<dyn ToolRegistry>,
    ) -> Self {
        self.llm_client = Some(llm_client);
        self.tool_registry = Some(tool_registry);
        self
    }

    /// Override the confidence threshold for LLM routing.
    pub fn with_llm_min_confidence(mut self, threshold: f32) -> Self {
        self.llm_min_confidence = threshold;
        self
    }

    /// Configure explicit routing policy engine.
    pub fn with_routing_policy(mut self, policy: RoutingPolicyEngine) -> Self {
        self.routing_policy = Some(Arc::new(policy));
        self
    }

    /// Configure shared, versioned policy store for runtime routing updates.
    pub fn with_routing_policy_store(mut self, store: SharedRoutingPolicyStore) -> Self {
        self.routing_policy_store = Some(store);
        self
    }

    /// Add a custom fast action pattern.
    pub fn with_fast_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.fast_patterns.push(pattern.into());
        self
    }

    /// Add a custom complex mission pattern.
    pub fn with_complex_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.complex_patterns.push(pattern.into());
        self
    }

    /// Check if content matches any fast action keywords.
    fn is_fast_action(&self, content: &str) -> bool {
        let lower = content.to_lowercase();

        for pattern in &self.fast_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                return true;
            }
        }

        for keyword in FAST_ACTION_KEYWORDS {
            if lower.starts_with(keyword) || lower.contains(&format!(" {}", keyword)) {
                return true;
            }
        }

        false
    }

    /// Check if content matches any complex mission keywords.
    fn is_complex_mission(&self, content: &str) -> bool {
        let lower = content.to_lowercase();

        for pattern in &self.complex_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                return true;
            }
        }

        for keyword in COMPLEX_MISSION_KEYWORDS {
            if lower.contains(keyword) {
                return true;
            }
        }

        false
    }

    /// Extract a goal from the content.
    fn extract_goal(&self, content: &str) -> String {
        let goal = content.split(['.', '!', '?']).next().unwrap_or(content);

        if goal.len() > 200 {
            format!("{}...", &goal[..200])
        } else {
            goal.to_string()
        }
    }

    /// Extract a tool name from fast action content.
    fn extract_tool_name(&self, content: &str) -> String {
        let lower = content.to_lowercase();

        if lower.contains("search") || lower.contains("find") {
            "search".to_string()
        } else if lower.contains("calculate") || lower.contains("compute") {
            "calculator".to_string()
        } else if lower.contains("read") || lower.contains("get") {
            "read_artifact".to_string()
        } else if lower.contains("list") || lower.contains("show") {
            "list".to_string()
        } else {
            "generic".to_string()
        }
    }

    fn extract_json_object(content: &str) -> Option<String> {
        if serde_json::from_str::<serde_json::Value>(content).is_ok() {
            return Some(content.to_string());
        }

        let start = content.find('{')?;
        let end = content.rfind('}')?;
        if end > start {
            Some(content[start..=end].to_string())
        } else {
            None
        }
    }

    fn build_llm_prompt(request: &NormalizedRequest, tool_context: &str) -> Vec<ChatMessage> {
        vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are an intent router. Return ONLY compact JSON with keys: intent_type (fast_action|complex_mission), tool_name (optional), args (optional object), goal (optional), confidence (0..1).".to_string(),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!(
                    "message: {}\nrefs_count: {}\navailable_tools: {}",
                    request.content,
                    request.refs.len(),
                    tool_context
                ),
                tool_calls: None,
            },
        ]
    }

    async fn classify_with_llm(
        &self,
        request: &NormalizedRequest,
    ) -> std::result::Result<(UserIntent, serde_json::Value), &'static str> {
        let llm = self.llm_client.as_ref().ok_or("llm_not_configured")?;
        let tool_registry = self
            .tool_registry
            .as_ref()
            .ok_or("tool_registry_not_configured")?;

        let tools = tool_registry
            .list()
            .await
            .map_err(|_| "tool_registry_list_failed")?;
        let tool_names: std::collections::HashSet<String> =
            tools.iter().map(|t| t.name.clone()).collect();
        let tool_context = tools
            .iter()
            .map(|t| format!("{}: {}", t.name, t.description))
            .collect::<Vec<_>>()
            .join(" | ");

        let messages = Self::build_llm_prompt(request, &tool_context);
        let response = llm.chat(&messages).await.map_err(|_| "llm_call_failed")?;
        let json_str = Self::extract_json_object(&response.content).ok_or("llm_invalid_json")?;
        let decision: LlmIntentDecision =
            serde_json::from_str(&json_str).map_err(|_| "llm_invalid_json")?;

        let confidence = decision.confidence.unwrap_or(0.0);
        if confidence < self.llm_min_confidence {
            tracing::debug!(
                confidence = confidence,
                "LLM intent below confidence threshold"
            );
            return Err("llm_low_confidence");
        }

        let user_id = request.metadata.user_id.clone();
        match decision.intent_type.as_str() {
            "fast_action" => {
                let tool_name = decision.tool_name.ok_or("llm_missing_tool_name")?;
                if !tool_names.contains(&tool_name) {
                    tracing::debug!(tool = %tool_name, "LLM selected unavailable tool; falling back");
                    return Err("llm_unknown_tool");
                }
                Ok((
                    UserIntent::FastAction {
                        tool_name,
                        args: decision
                            .args
                            .unwrap_or_else(|| json!({ "query": request.content })),
                        user_id,
                    },
                    serde_json::json!({
                        "routing": {
                            "source": "llm",
                            "confidence": confidence,
                            "fallback_reason": serde_json::Value::Null
                        }
                    }),
                ))
            }
            "complex_mission" => Ok((
                UserIntent::ComplexMission {
                    goal: decision
                        .goal
                        .unwrap_or_else(|| self.extract_goal(&request.content)),
                    context_summary: request.content.clone(),
                    visual_refs: request.refs.iter().map(|r| r.0.clone()).collect(),
                    user_id,
                },
                serde_json::json!({
                    "routing": {
                        "source": "llm",
                        "confidence": confidence,
                        "fallback_reason": serde_json::Value::Null
                    }
                }),
            )),
            _ => Err("llm_unusable_decision"),
        }
    }

    fn classify_with_rules(&self, request: &NormalizedRequest) -> UserIntent {
        let content = &request.content;
        let user_id = request.metadata.user_id.clone();

        if !request.refs.is_empty() {
            return UserIntent::ComplexMission {
                goal: self.extract_goal(content),
                context_summary: content.clone(),
                visual_refs: request.refs.iter().map(|r| r.0.clone()).collect(),
                user_id,
            };
        }

        if self.is_complex_mission(content) {
            return UserIntent::ComplexMission {
                goal: self.extract_goal(content),
                context_summary: content.clone(),
                visual_refs: Vec::new(),
                user_id,
            };
        }

        if self.is_fast_action(content) {
            return UserIntent::FastAction {
                tool_name: self.extract_tool_name(content),
                args: json!({ "query": content }),
                user_id,
            };
        }

        UserIntent::ComplexMission {
            goal: self.extract_goal(content),
            context_summary: content.clone(),
            visual_refs: Vec::new(),
            user_id,
        }
    }

    fn routing_context_from_request(request: &NormalizedRequest) -> RoutingContext {
        RoutingContext {
            channel: request.metadata.custom.get("channel").cloned(),
            account: request.metadata.custom.get("account").cloned(),
            peer: request.metadata.custom.get("peer").cloned(),
        }
    }

    fn scope_label(scope: RouteScope) -> &'static str {
        match scope {
            RouteScope::Channel => "channel",
            RouteScope::Account => "account",
            RouteScope::Peer => "peer",
        }
    }

    async fn classify_with_policy(
        &self,
        request: &NormalizedRequest,
    ) -> Option<(UserIntent, serde_json::Value)> {
        let context = Self::routing_context_from_request(request);
        let requested_channel = request
            .metadata
            .custom
            .get("routing_channel")
            .map(|v| v.to_ascii_lowercase());
        let decision = if let Some(store) = &self.routing_policy_store {
            match requested_channel.as_deref() {
                Some("canary") => {
                    store
                        .resolve_for_channel(&context, RoutingPolicyChannel::Canary)
                        .await
                }
                Some("stable") => {
                    store
                        .resolve_for_channel(&context, RoutingPolicyChannel::Stable)
                        .await
                }
                _ => store.resolve(&context).await,
            }
        } else {
            self.routing_policy
                .as_ref()
                .and_then(|policy| policy.resolve(&context))
        }?;
        let user_id = request.metadata.user_id.clone();

        let intent = match decision.target {
            RouteTarget::FastAction { tool_name } => UserIntent::FastAction {
                tool_name,
                args: json!({ "query": request.content }),
                user_id,
            },
            RouteTarget::ComplexMission { goal_hint } => UserIntent::ComplexMission {
                goal: format!("{}: {}", goal_hint, self.extract_goal(&request.content)),
                context_summary: request.content.clone(),
                visual_refs: request.refs.iter().map(|r| r.0.clone()).collect(),
                user_id,
            },
        };

        Some((
            intent,
            serde_json::json!({
                "routing": {
                    "source": "policy",
                    "scope": Self::scope_label(decision.scope),
                    "rule_id": decision.rule_id,
                    "channel": requested_channel.unwrap_or_else(|| "default".to_string()),
                    "confidence": serde_json::Value::Null,
                    "fallback_reason": serde_json::Value::Null
                }
            }),
        ))
    }
}

impl Default for DefaultRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IntentRouter for DefaultRouter {
    async fn classify(&self, request: &NormalizedRequest) -> Result<UserIntent> {
        Ok(self.classify_detailed(request).await?.0)
    }

    async fn classify_detailed(
        &self,
        request: &NormalizedRequest,
    ) -> Result<(UserIntent, serde_json::Value)> {
        tracing::debug!(
            trace_id = %request.trace_id,
            content_length = request.content.len(),
            user_id = ?request.metadata.user_id,
            "Classifying intent"
        );

        if let Some((intent, diagnostics)) = self.classify_with_policy(request).await {
            tracing::debug!("Intent classified via explicit routing policy");
            return Ok((intent, diagnostics));
        }

        match self.classify_with_llm(request).await {
            Ok((intent, diagnostics)) => {
                tracing::debug!("Intent classified via LLM");
                Ok((intent, diagnostics))
            }
            Err(fallback_reason) => {
                tracing::debug!(
                    fallback_reason = fallback_reason,
                    "Intent classified via fallback rules"
                );
                Ok((
                    self.classify_with_rules(request),
                    serde_json::json!({
                        "routing": {
                            "source": "fallback_rules",
                            "confidence": serde_json::Value::Null,
                            "fallback_reason": fallback_reason
                        }
                    }),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing_policy::{RouteScope, RoutingPolicyEngine, RoutingRule};
    use multi_agent_core::traits::{LlmResponse, LlmUsage, Tool, ToolRegistry};
    use multi_agent_core::types::{ToolDefinition, ToolOutput};
    use serde_json::Value;

    struct MockLlm {
        response: String,
    }

    #[async_trait]
    impl LlmClient for MockLlm {
        async fn complete(&self, _prompt: &str) -> multi_agent_core::Result<LlmResponse> {
            Ok(LlmResponse {
                content: self.response.clone(),
                finish_reason: "stop".to_string(),
                usage: LlmUsage::default(),
                tool_calls: None,
            })
        }

        async fn chat(&self, _messages: &[ChatMessage]) -> multi_agent_core::Result<LlmResponse> {
            Ok(LlmResponse {
                content: self.response.clone(),
                finish_reason: "stop".to_string(),
                usage: LlmUsage::default(),
                tool_calls: None,
            })
        }

        async fn embed(&self, _text: &str) -> multi_agent_core::Result<Vec<f32>> {
            Ok(vec![0.0; 8])
        }
    }

    struct MockRegistry {
        tools: Vec<ToolDefinition>,
    }

    #[async_trait]
    impl ToolRegistry for MockRegistry {
        async fn register(&self, _tool: Box<dyn Tool>) -> multi_agent_core::Result<()> {
            Ok(())
        }

        async fn get(&self, _name: &str) -> multi_agent_core::Result<Option<Box<dyn Tool>>> {
            Ok(None)
        }

        async fn list(&self) -> multi_agent_core::Result<Vec<ToolDefinition>> {
            Ok(self.tools.clone())
        }

        async fn execute(&self, _name: &str, _args: Value) -> multi_agent_core::Result<ToolOutput> {
            Ok(ToolOutput::error("not implemented"))
        }
    }

    #[tokio::test]
    async fn test_fast_action_classification() {
        let router = DefaultRouter::new();

        let request = NormalizedRequest::text("search for Rust async patterns");
        let intent = router.classify(&request).await.unwrap();

        match intent {
            UserIntent::FastAction { tool_name, .. } => {
                assert_eq!(tool_name, "search");
            }
            _ => panic!("Expected FastAction"),
        }
    }

    #[tokio::test]
    async fn test_complex_mission_classification() {
        let router = DefaultRouter::new();

        let request = NormalizedRequest::text("Help me build a REST API in Rust");
        let intent = router.classify(&request).await.unwrap();

        match intent {
            UserIntent::ComplexMission { goal, .. } => {
                assert!(goal.contains("Help me build"));
            }
            _ => panic!("Expected ComplexMission"),
        }
    }

    #[tokio::test]
    async fn test_refs_force_complex() {
        use multi_agent_core::types::RefId;

        let router = DefaultRouter::new();

        let request =
            NormalizedRequest::text("What is this?").with_ref(RefId::from_string("image_123"));

        let intent = router.classify(&request).await.unwrap();

        match intent {
            UserIntent::ComplexMission { visual_refs, .. } => {
                assert_eq!(visual_refs.len(), 1);
                assert_eq!(visual_refs[0], "image_123");
            }
            _ => panic!("Expected ComplexMission due to refs"),
        }
    }

    #[tokio::test]
    async fn test_llm_routing_fast_action_known_tool() {
        let llm = Arc::new(MockLlm {
            response: r#"{"intent_type":"fast_action","tool_name":"search","args":{"query":"rust"},"confidence":0.95}"#.to_string(),
        });
        let registry = Arc::new(MockRegistry {
            tools: vec![ToolDefinition {
                name: "search".to_string(),
                description: "search web".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                supports_streaming: false,
            }],
        });

        let router = DefaultRouter::new().with_llm_classifier(llm, registry);
        let request = NormalizedRequest::text("find rust async examples");
        let intent = router.classify(&request).await.unwrap();

        match intent {
            UserIntent::FastAction { tool_name, .. } => assert_eq!(tool_name, "search"),
            _ => panic!("Expected FastAction"),
        }
    }

    #[tokio::test]
    async fn test_llm_unknown_tool_falls_back() {
        let llm = Arc::new(MockLlm {
            response: r#"{"intent_type":"fast_action","tool_name":"nonexistent","args":{"query":"rust"},"confidence":0.99}"#.to_string(),
        });
        let registry = Arc::new(MockRegistry {
            tools: vec![ToolDefinition {
                name: "search".to_string(),
                description: "search web".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                supports_streaming: false,
            }],
        });

        let router = DefaultRouter::new().with_llm_classifier(llm, registry);
        let request = NormalizedRequest::text("search rust ownership");
        let intent = router.classify(&request).await.unwrap();

        match intent {
            UserIntent::FastAction { tool_name, .. } => assert_eq!(tool_name, "search"),
            _ => panic!("Expected FastAction via fallback"),
        }
    }

    #[tokio::test]
    async fn test_llm_low_confidence_falls_back() {
        let llm = Arc::new(MockLlm {
            response: r#"{"intent_type":"fast_action","tool_name":"search","args":{"query":"rust"},"confidence":0.20}"#.to_string(),
        });
        let registry = Arc::new(MockRegistry {
            tools: vec![ToolDefinition {
                name: "search".to_string(),
                description: "search web".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                supports_streaming: false,
            }],
        });

        let router = DefaultRouter::new().with_llm_classifier(llm, registry);
        let request = NormalizedRequest::text("search rust lifetimes");
        let intent = router.classify(&request).await.unwrap();

        match intent {
            UserIntent::FastAction { tool_name, .. } => assert_eq!(tool_name, "search"),
            _ => panic!("Expected FastAction via fallback"),
        }
    }

    #[tokio::test]
    async fn test_llm_diagnostics_contains_source_and_confidence() {
        let llm = Arc::new(MockLlm {
            response: r#"{"intent_type":"fast_action","tool_name":"search","args":{"query":"rust"},"confidence":0.91}"#.to_string(),
        });
        let registry = Arc::new(MockRegistry {
            tools: vec![ToolDefinition {
                name: "search".to_string(),
                description: "search web".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                supports_streaming: false,
            }],
        });
        let router = DefaultRouter::new().with_llm_classifier(llm, registry);
        let request = NormalizedRequest::text("search rust trait object");

        let (_, diagnostics) = router.classify_detailed(&request).await.unwrap();
        assert_eq!(diagnostics["routing"]["source"], "llm");
        let conf = diagnostics["routing"]["confidence"].as_f64().unwrap();
        assert!((conf - 0.91).abs() < 1e-3);
    }

    #[tokio::test]
    async fn test_fallback_diagnostics_contains_reason() {
        let llm = Arc::new(MockLlm {
            response:
                r#"{"intent_type":"fast_action","tool_name":"missing_tool","confidence":0.95}"#
                    .to_string(),
        });
        let registry = Arc::new(MockRegistry {
            tools: vec![ToolDefinition {
                name: "search".to_string(),
                description: "search web".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                supports_streaming: false,
            }],
        });
        let router = DefaultRouter::new().with_llm_classifier(llm, registry);
        let request = NormalizedRequest::text("search rust async");

        let (_, diagnostics) = router.classify_detailed(&request).await.unwrap();
        assert_eq!(diagnostics["routing"]["source"], "fallback_rules");
        assert_eq!(
            diagnostics["routing"]["fallback_reason"],
            "llm_unknown_tool"
        );
    }

    #[tokio::test]
    async fn test_policy_forces_fast_action_before_llm() {
        let llm = Arc::new(MockLlm {
            response: r#"{"intent_type":"complex_mission","goal":"llm-route","confidence":0.99}"#
                .to_string(),
        });
        let registry = Arc::new(MockRegistry {
            tools: vec![ToolDefinition {
                name: "search".to_string(),
                description: "search web".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                supports_streaming: false,
            }],
        });
        let policy = RoutingPolicyEngine::new(vec![RoutingRule::force_fast(
            "channel-fast",
            RouteScope::Channel,
            "support",
            "search",
            1,
        )]);

        let router = DefaultRouter::new()
            .with_llm_classifier(llm, registry)
            .with_routing_policy(policy);
        let mut request = NormalizedRequest::text("please help");
        request
            .metadata
            .custom
            .insert("channel".to_string(), "support".to_string());

        let (intent, diagnostics) = router.classify_detailed(&request).await.unwrap();
        match intent {
            UserIntent::FastAction { tool_name, .. } => assert_eq!(tool_name, "search"),
            _ => panic!("Expected FastAction"),
        }
        assert_eq!(diagnostics["routing"]["source"], "policy");
        assert_eq!(diagnostics["routing"]["scope"], "channel");
        assert_eq!(diagnostics["routing"]["rule_id"], "channel-fast");
    }

    #[tokio::test]
    async fn test_policy_forces_complex_mission() {
        let policy = RoutingPolicyEngine::new(vec![RoutingRule::force_complex(
            "peer-complex",
            RouteScope::Peer,
            "vip-user",
            "Escalated workflow",
            5,
        )]);
        let router = DefaultRouter::new().with_routing_policy(policy);

        let mut request = NormalizedRequest::text("search docs");
        request
            .metadata
            .custom
            .insert("peer".to_string(), "vip-user".to_string());

        let (intent, diagnostics) = router.classify_detailed(&request).await.unwrap();
        match intent {
            UserIntent::ComplexMission { goal, .. } => {
                assert!(goal.contains("Escalated workflow"));
            }
            _ => panic!("Expected ComplexMission"),
        }
        assert_eq!(diagnostics["routing"]["source"], "policy");
        assert_eq!(diagnostics["routing"]["scope"], "peer");
        assert_eq!(diagnostics["routing"]["rule_id"], "peer-complex");
    }
}
