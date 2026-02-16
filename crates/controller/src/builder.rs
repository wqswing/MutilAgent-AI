//! Builder for ReActController.

use multi_agent_core::traits::{
    ApprovalGate, ArtifactStore, LlmClient, SessionStore, ToolRegistry,
};
use multi_agent_governance::Guardrail;
use std::sync::Arc;

use crate::capability::{
    AgentCapability, CompressionCapability, DelegationCapability, McpCapability,
    ReflectionCapability, SecurityCapability,
};
use crate::context::{CompressionConfig, ContextCompressor};
use crate::delegation::Delegator;
use crate::react::{ReActConfig, ReActController};
use crate::{MemoryCapability, PlanningCapability};

/// Builder for constructing a ReActController.
pub struct ReActBuilder {
    config: ReActConfig,
    llm: Option<Arc<dyn LlmClient>>,
    tools: Option<Arc<dyn ToolRegistry>>,
    store: Option<Arc<dyn ArtifactStore>>,
    session_store: Option<Arc<dyn SessionStore>>,
    compression_config: CompressionConfig,
    capabilities: Vec<Arc<dyn AgentCapability>>,
    approval_gate: Option<Arc<dyn ApprovalGate>>,
    policy_engine: Option<Arc<tokio::sync::RwLock<multi_agent_governance::PolicyEngine>>>,
    event_emitter: Option<Arc<dyn multi_agent_core::traits::EventEmitter>>,
}

impl ReActBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: ReActConfig::default(),
            llm: None,
            tools: None,
            store: None,
            session_store: None,
            compression_config: CompressionConfig::default(),
            capabilities: Vec::new(),
            approval_gate: None,
            policy_engine: None,
            event_emitter: None,
        }
    }

    /// Set the configuration.
    pub fn with_config(mut self, config: ReActConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the LLM client.
    pub fn with_llm(mut self, llm: Arc<dyn LlmClient>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Set the tool registry.
    pub fn with_tools(mut self, tools: Arc<dyn ToolRegistry>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the artifact store.
    pub fn with_store(mut self, store: Arc<dyn ArtifactStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Set the session store.
    pub fn with_session_store(mut self, session_store: Arc<dyn SessionStore>) -> Self {
        self.session_store = Some(session_store);
        self
    }

    /// Set the context compressor (compatibility mode).
    pub fn with_compressor(mut self, compressor: Arc<dyn ContextCompressor>) -> Self {
        let cap = CompressionCapability::new(compressor, self.compression_config.clone());
        self.capabilities.push(Arc::new(cap));
        self
    }

    /// Set compression configuration AND update an existing CompressionCapability (compatibility mode).
    pub fn with_compression_config(mut self, config: CompressionConfig) -> Self {
        self.compression_config = config;
        self
    }

    /// Set the delegator for subagent spawning (compatibility mode).
    pub fn with_delegator(mut self, delegator: Arc<dyn Delegator>) -> Self {
        self.capabilities
            .push(Arc::new(DelegationCapability::new(delegator)));
        self
    }

    /// Set the MCP registry for autonomous server selection (compatibility mode).
    pub fn with_mcp_registry(mut self, registry: Arc<multi_agent_skills::McpRegistry>) -> Self {
        self.capabilities
            .push(Arc::new(McpCapability::new(registry)));
        self
    }

    /// Set security guardrails for input/output validation (compatibility mode).
    pub fn with_security(mut self, security: Arc<dyn Guardrail>) -> Self {
        self.capabilities
            .push(Arc::new(SecurityCapability::new(security)));
        self
    }

    /// Set reflection capability for self-correction (compatibility mode).
    pub fn with_reflection(mut self, threshold: usize) -> Self {
        self.capabilities
            .push(Arc::new(ReflectionCapability::new(threshold)));
        self
    }

    /// Set long-term memory for RAG (compatibility mode).
    pub fn with_memory(
        mut self,
        store: Arc<dyn multi_agent_core::traits::MemoryStore>,
        llm: Arc<dyn multi_agent_core::traits::LlmClient>,
    ) -> Self {
        self.capabilities
            .push(Arc::new(MemoryCapability::new(store, llm, 5, 0.7)));
        self
    }

    /// Set plan-and-solve capability (compatibility mode).
    pub fn with_planning(mut self, llm: Arc<dyn multi_agent_core::traits::LlmClient>) -> Self {
        self.capabilities
            .push(Arc::new(PlanningCapability::new(llm)));
        self
    }

    /// Add a generic capability (plugin architecture).
    pub fn with_capability(mut self, capability: Arc<dyn AgentCapability>) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// Set the HITL approval gate for high-risk tool execution.
    pub fn with_approval_gate(mut self, gate: Arc<dyn ApprovalGate>) -> Self {
        self.approval_gate = Some(gate);
        self
    }

    /// Set the event emitter for structured events.
    pub fn with_event_emitter(
        mut self,
        emitter: Arc<dyn multi_agent_core::traits::EventEmitter>,
    ) -> Self {
        self.event_emitter = Some(emitter);
        self
    }

    /// Set the Policy Engine for rule-based risk assessment.
    pub fn with_policy_engine(
        mut self,
        engine: Arc<tokio::sync::RwLock<multi_agent_governance::PolicyEngine>>,
    ) -> Self {
        self.policy_engine = Some(engine);
        self
    }

    /// Build the ReActController.
    pub fn build(self) -> ReActController {
        ReActController {
            config: self.config,
            llm: self.llm,
            tools: self.tools,
            // store is currently unused in Controller, dropped
            session_store: self.session_store,
            // compression_config is used to configure capabilities, not stored in Controller
            capabilities: self.capabilities,
            approval_gate: self.approval_gate,
            policy_engine: self.policy_engine,
            event_emitter: self.event_emitter,
        }
    }
}

impl Default for ReActBuilder {
    fn default() -> Self {
        Self::new()
    }
}
