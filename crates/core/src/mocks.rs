//! Mock implementations of core traits for testing.
//!
//! This module provides mock implementations of all core traits that can be
//! used across the codebase for comprehensive unit and integration testing.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde_json::Value;

use crate::{
    traits::{
        LlmClient, LlmResponse, LlmUsage, ChatMessage,
        MemoryStore, MemoryEntry,
        ToolRegistry, Tool,
        IntentRouter, SemanticCache,
        SessionStore,
    },
    types::{UserIntent, NormalizedRequest, Session, ToolOutput, ToolDefinition},
    Result, Error,
};

// =============================================================================
// Mock LLM Client
// =============================================================================

/// Scripted mock LLM that returns predefined responses.
pub struct MockLlm {
    responses: Mutex<Vec<String>>,
    call_count: Mutex<usize>,
}

impl MockLlm {
    /// Create a new mock LLM with a queue of responses.
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(responses),
            call_count: Mutex::new(0),
        }
    }

    /// Create a mock that always returns the same response.
    pub fn constant(response: &str) -> Self {
        Self::new(vec![response.to_string()])
    }

    /// Get the number of calls made to this mock.
    pub fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl LlmClient for MockLlm {
    async fn complete(&self, _prompt: &str) -> Result<LlmResponse> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        
        let responses = self.responses.lock().unwrap();
        let idx = (*count - 1) % responses.len().max(1);
        let content = responses.get(idx).cloned().unwrap_or_else(|| "FINAL ANSWER: Done".to_string());
        
        Ok(LlmResponse {
            content,
            finish_reason: "stop".to_string(),
            usage: LlmUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            },
            tool_calls: None,
        })
    }

    async fn chat(&self, _messages: &[ChatMessage]) -> Result<LlmResponse> {
        self.complete("").await
    }

    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        // Return a simple normalized embedding
        Ok(vec![0.5; 1536])
    }
}

// =============================================================================
// Mock Memory Store
// =============================================================================

/// In-memory mock for MemoryStore trait.
#[derive(Default)]
pub struct MockMemoryStore {
    entries: Mutex<HashMap<String, MemoryEntry>>,
}

impl MockMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the store with entries.
    pub fn with_entries(entries: Vec<MemoryEntry>) -> Self {
        let store = Self::new();
        {
            let mut map = store.entries.lock().unwrap();
            for entry in entries {
                map.insert(entry.id.clone(), entry);
            }
        }
        store
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl MemoryStore for MockMemoryStore {
    async fn add(&self, entry: MemoryEntry) -> Result<()> {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(entry.id.clone(), entry);
        Ok(())
    }

    async fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<MemoryEntry>> {
        let entries = self.entries.lock().unwrap();
        let mut results: Vec<_> = entries.values().cloned().collect();
        
        // Simple similarity scoring based on first dimension
        results.sort_by(|a, b| {
            let sim_a = if !a.embedding.is_empty() && !query_embedding.is_empty() {
                (a.embedding[0] - query_embedding[0]).abs()
            } else {
                f32::MAX
            };
            let sim_b = if !b.embedding.is_empty() && !query_embedding.is_empty() {
                (b.embedding[0] - query_embedding[0]).abs()
            } else {
                f32::MAX
            };
            sim_a.partial_cmp(&sim_b).unwrap()
        });
        
        results.truncate(limit);
        Ok(results)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(id);
        Ok(())
    }
}

// =============================================================================
// Mock Tool Registry
// =============================================================================

/// Mock tool that records calls.
pub struct RecordingTool {
    name: String,
    description: String,
    response: String,
    calls: Mutex<Vec<Value>>,
}

impl RecordingTool {
    pub fn new(name: &str, description: &str, response: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            response: response.to_string(),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl Tool for RecordingTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        self.calls.lock().unwrap().push(args);
        Ok(ToolOutput {
            success: true,
            content: self.response.clone(),
            data: None,
            created_refs: Vec::new(),
        })
    }
}

/// Simple mock tool registry.
#[derive(Default)]
pub struct MockToolRegistry {
    tools: Mutex<HashMap<String, Arc<dyn Tool>>>,
}

impl MockToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with predefined tools.
    pub fn with_tools(tools: Vec<Arc<dyn Tool>>) -> Self {
        let registry = Self::new();
        {
            let mut map = registry.tools.lock().unwrap();
            for tool in tools {
                map.insert(tool.name().to_string(), tool);
            }
        }
        registry
    }
}

#[async_trait]
impl ToolRegistry for MockToolRegistry {
    async fn register(&self, tool: Box<dyn Tool>) -> Result<()> {
        let mut tools = self.tools.lock().unwrap();
        tools.insert(tool.name().to_string(), Arc::from(tool));
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<Box<dyn Tool>>> {
        // Cannot easily clone a Box<dyn Tool>, return None for mock
        let tools = self.tools.lock().unwrap();
        Ok(if tools.contains_key(name) { None } else { None })
    }

    async fn list(&self) -> Result<Vec<ToolDefinition>> {
        let tools = self.tools.lock().unwrap();
        Ok(tools.values().map(|t| ToolDefinition {
            name: t.name().to_string(),
            description: t.description().to_string(),
            parameters: t.parameters(),
            supports_streaming: false,
        }).collect())
    }

    async fn execute(&self, name: &str, args: Value) -> Result<ToolOutput> {
        // Clone the tool before releasing the lock to avoid holding lock across await
        let tool = {
            let tools = self.tools.lock().unwrap();
            tools.get(name).cloned()
        };
        
        if let Some(tool) = tool {
            tool.execute(args).await
        } else {
            Err(Error::controller(format!("Tool not found: {}", name)))
        }
    }
}

// =============================================================================
// Mock Intent Router
// =============================================================================

/// Mock router that returns a fixed intent.
pub struct MockRouter {
    intent: UserIntent,
}

impl MockRouter {
    pub fn new(intent: UserIntent) -> Self {
        Self { intent }
    }

    /// Create a router that always returns ComplexMission.
    pub fn complex_mission(goal: &str) -> Self {
        Self::new(UserIntent::ComplexMission { 
            goal: goal.to_string(),
            context_summary: String::new(),
            visual_refs: Vec::new(),
        })
    }

    /// Create a router that always returns FastAction.
    pub fn fast_action(tool: &str, args: Value) -> Self {
        Self::new(UserIntent::FastAction { tool_name: tool.to_string(), args })
    }
}

#[async_trait]
impl IntentRouter for MockRouter {
    async fn classify(&self, _request: &NormalizedRequest) -> Result<UserIntent> {
        Ok(self.intent.clone())
    }
}

// =============================================================================
// Mock Semantic Cache
// =============================================================================

/// Mock semantic cache.
#[derive(Default)]
pub struct MockSemanticCache {
    cache: Mutex<HashMap<String, String>>,
}

impl MockSemanticCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with preset cache entries.
    pub fn with_entries(entries: Vec<(&str, &str)>) -> Self {
        let cache = Self::new();
        {
            let mut map = cache.cache.lock().unwrap();
            for (k, v) in entries {
                map.insert(k.to_string(), v.to_string());
            }
        }
        cache
    }
}

#[async_trait]
impl SemanticCache for MockSemanticCache {
    async fn get(&self, query: &str) -> Result<Option<String>> {
        let cache = self.cache.lock().unwrap();
        Ok(cache.get(query).cloned())
    }

    async fn set(&self, query: &str, response: &str) -> Result<()> {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(query.to_string(), response.to_string());
        Ok(())
    }

    async fn invalidate(&self, pattern: &str) -> Result<()> {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|k, _| !k.contains(pattern));
        Ok(())
    }
}

// =============================================================================
// Mock Session Store
// =============================================================================

/// In-memory mock for SessionStore.
#[derive(Default)]
pub struct MockSessionStore {
    sessions: Mutex<HashMap<String, Session>>,
}

impl MockSessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for MockSessionStore {
    async fn save(&self, session: &Session) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn load(&self, session_id: &str) -> Result<Option<Session>> {
        let sessions = self.sessions.lock().unwrap();
        Ok(sessions.get(session_id).cloned())
    }

    async fn delete(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.remove(session_id);
        Ok(())
    }

    async fn list_running(&self) -> Result<Vec<String>> {
        let sessions = self.sessions.lock().unwrap();
        Ok(sessions.iter()
            .filter(|(_, s)| s.status == crate::types::SessionStatus::Running)
            .map(|(id, _)| id.clone())
            .collect())
    }
}

// Tests moved to integration tests to avoid tokio dependency in core
