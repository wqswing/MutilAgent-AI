//! Parser module for LLM response parsing.
//!
//! Extracts structured actions (ToolCall, FinalAnswer, etc.) from raw LLM text.

use crate::capability::AgentCapability;
use std::sync::Arc;

/// Parsed action from LLM response.
#[derive(Debug, Clone)]
pub enum ReActAction {
    /// Call a tool with arguments.
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    /// Final answer - task complete.
    FinalAnswer(String),
    /// Continue thinking (no action yet).
    Think(String),
    /// Delegate to a subagent (v0.2 autonomous capability).
    Delegate {
        objective: String,
        context: String,
    },
    /// Select MCP server for a task (v0.2 autonomous capability).
    McpSelect {
        task_description: String,
    },
}

/// Parser for LLM responses, supporting multiple formats.
pub struct ActionParser {
    /// Registered capabilities for custom action parsing.
    capabilities: Vec<Arc<dyn AgentCapability>>,
}

impl ActionParser {
    /// Create a new parser with the given capabilities.
    pub fn new(capabilities: Vec<Arc<dyn AgentCapability>>) -> Self {
        Self { capabilities }
    }

    /// Parse an LLM response into a structured action.
    pub fn parse(&self, response: &str) -> ReActAction {
        let response_trimmed = response.trim();

        // 1. Check capabilities for custom actions (Delegation, MCP, etc.)
        for cap in &self.capabilities {
            if let Some(action) = cap.parse_action(response_trimmed) {
                return action;
            }
        }

        // 2. Check for FINAL ANSWER
        if let Some(answer) = response_trimmed.strip_prefix("FINAL ANSWER:") {
            return ReActAction::FinalAnswer(answer.trim().to_string());
        }

        // 3. Try parsing OpenAI-style function call JSON (tool_calls)
        if let Some(action) = self.try_parse_function_call(response_trimmed) {
            return action;
        }

        // 4. Check for ACTION + ARGS pattern (Custom text format)
        if let Some(action) = self.try_parse_text_format(response_trimmed) {
            return action;
        }

        // Default: treat as thought
        ReActAction::Think(response_trimmed.to_string())
    }

    /// Try to parse OpenAI-style function call from JSON.
    fn try_parse_function_call(&self, response: &str) -> Option<ReActAction> {
        // Look for tool_calls in the response (common in structured output)
        if response.starts_with('{') || response.starts_with('[') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(response) {
                // Handle array of tool calls
                if let Some(calls) = json.as_array() {
                    if let Some(first_call) = calls.first() {
                        return self.extract_tool_call(first_call);
                    }
                }
                // Handle single object with "function" or "name"
                return self.extract_tool_call(&json);
            }
        }
        None
    }

    /// Extract tool call from a JSON object.
    fn extract_tool_call(&self, json: &serde_json::Value) -> Option<ReActAction> {
        // OpenAI format: { "function": { "name": "...", "arguments": "..." } }
        if let Some(func) = json.get("function") {
            let name = func.get("name")?.as_str()?.to_string();
            let args_str = func.get("arguments")?.as_str()?;
            let args = serde_json::from_str(args_str).ok()?;
            return Some(ReActAction::ToolCall { name, args });
        }

        // Simple format: { "name": "...", "arguments": {...} }
        if let Some(name) = json.get("name").and_then(|n| n.as_str()) {
            let args = json.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            return Some(ReActAction::ToolCall {
                name: name.to_string(),
                args,
            });
        }

        None
    }

    /// Try to parse ACTION/ARGS text format.
    fn try_parse_text_format(&self, response: &str) -> Option<ReActAction> {
        let lines: Vec<&str> = response.lines().collect();
        let mut tool_name = None;
        let mut args_json = None;

        for line in lines {
            if line.starts_with("ACTION:") {
                tool_name = Some(line.trim_start_matches("ACTION:").trim().to_string());
            } else if line.starts_with("ARGS:") {
                args_json = Some(line.trim_start_matches("ARGS:").trim().to_string());
            }
        }

        if let (Some(name), Some(args_str)) = (tool_name, args_json) {
            if let Ok(args) = serde_json::from_str::<serde_json::Value>(&args_str) {
                return Some(ReActAction::ToolCall { name, args });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_final_answer() {
        let parser = ActionParser::new(vec![]);
        let action = parser.parse("FINAL ANSWER: The result is 42.");
        match action {
            ReActAction::FinalAnswer(answer) => assert!(answer.contains("42")),
            _ => panic!("Expected FinalAnswer"),
        }
    }

    #[test]
    fn test_parse_text_tool_call() {
        let parser = ActionParser::new(vec![]);
        let action = parser.parse("THOUGHT: I need to search.\nACTION: search\nARGS: {\"query\": \"rust\"}");
        match action {
            ReActAction::ToolCall { name, args } => {
                assert_eq!(name, "search");
                assert_eq!(args["query"], "rust");
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_parse_json_function_call() {
        let parser = ActionParser::new(vec![]);
        let action = parser.parse(r#"{"name": "calculator", "arguments": {"a": 5, "b": 3}}"#);
        match action {
            ReActAction::ToolCall { name, args } => {
                assert_eq!(name, "calculator");
                assert_eq!(args["a"], 5);
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_parse_think() {
        let parser = ActionParser::new(vec![]);
        let action = parser.parse("I'm still thinking about this problem...");
        match action {
            ReActAction::Think(thought) => assert!(thought.contains("thinking")),
            _ => panic!("Expected Think"),
        }
    }
}
