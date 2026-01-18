//! Intent router for classifying incoming requests.

use async_trait::async_trait;
use serde_json::json;

use multi_agent_core::{
    traits::IntentRouter,
    types::{NormalizedRequest, UserIntent}, Result,
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

/// Default router implementation using keyword-based classification.
///
/// In a production system, this would use an LLM (e.g., GPT-4o-mini)
/// for more accurate intent classification.
pub struct DefaultRouter {
    /// Custom fast action patterns.
    fast_patterns: Vec<String>,
    /// Custom complex mission patterns.
    complex_patterns: Vec<String>,
}

impl DefaultRouter {
    /// Create a new default router.
    pub fn new() -> Self {
        Self {
            fast_patterns: Vec::new(),
            complex_patterns: Vec::new(),
        }
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

        // Check custom patterns first
        for pattern in &self.fast_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                return true;
            }
        }

        // Check default keywords
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

        // Check custom patterns first
        for pattern in &self.complex_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                return true;
            }
        }

        // Check default keywords
        for keyword in COMPLEX_MISSION_KEYWORDS {
            if lower.contains(keyword) {
                return true;
            }
        }

        // If there are visual references, it's likely complex
        false
    }

    /// Extract a goal from the content.
    fn extract_goal(&self, content: &str) -> String {
        // Simple extraction: use the first sentence or first 200 chars
        let goal = content
            .split(['.', '!', '?'])
            .next()
            .unwrap_or(content);

        if goal.len() > 200 {
            format!("{}...", &goal[..200])
        } else {
            goal.to_string()
        }
    }

    /// Extract a tool name from fast action content.
    fn extract_tool_name(&self, content: &str) -> String {
        let lower = content.to_lowercase();

        // Map common intents to tools
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
}

impl Default for DefaultRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IntentRouter for DefaultRouter {
    async fn classify(&self, request: &NormalizedRequest) -> Result<UserIntent> {
        let content = &request.content;

        tracing::debug!(
            trace_id = %request.trace_id,
            content_length = content.len(),
            "Classifying intent"
        );

        // Check for visual references - complex by default
        if !request.refs.is_empty() {
            tracing::debug!(
                refs_count = request.refs.len(),
                "Request has references, routing as ComplexMission"
            );
            return Ok(UserIntent::ComplexMission {
                goal: self.extract_goal(content),
                context_summary: content.clone(),
                visual_refs: request.refs.iter().map(|r| r.0.clone()).collect(),
            });
        }

        // Check for complex mission first (higher priority)
        if self.is_complex_mission(content) {
            tracing::debug!("Routing as ComplexMission based on keywords");
            return Ok(UserIntent::ComplexMission {
                goal: self.extract_goal(content),
                context_summary: content.clone(),
                visual_refs: Vec::new(),
            });
        }

        // Check for fast action
        if self.is_fast_action(content) {
            tracing::debug!("Routing as FastAction based on keywords");
            return Ok(UserIntent::FastAction {
                tool_name: self.extract_tool_name(content),
                args: json!({ "query": content }),
            });
        }

        // Default to complex mission for ambiguous requests
        tracing::debug!("Defaulting to ComplexMission for ambiguous request");
        Ok(UserIntent::ComplexMission {
            goal: self.extract_goal(content),
            context_summary: content.clone(),
            visual_refs: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let request = NormalizedRequest::text("What is this?")
            .with_ref(RefId::from_string("image_123"));

        let intent = router.classify(&request).await.unwrap();

        match intent {
            UserIntent::ComplexMission { visual_refs, .. } => {
                assert_eq!(visual_refs.len(), 1);
                assert_eq!(visual_refs[0], "image_123");
            }
            _ => panic!("Expected ComplexMission due to refs"),
        }
    }
}
