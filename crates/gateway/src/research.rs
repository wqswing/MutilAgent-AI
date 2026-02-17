use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use multi_agent_core::{
    events::{EventEnvelope, EventType},
    types::research::ResearchPlan,
    traits::{ApprovalGate, ArtifactStore, KnowledgeStore, KnowledgeEntry},
    Result, Error,
};
use multi_agent_governance::{
    approval::ChannelApprovalGate,
    network::{NetworkPolicy, NetworkDecision},
};
use multi_agent_admin::AdminState;
use rig::prelude::*;
use rig::providers::openai;
use rig::completion::Prompt;
use chrono::Utc;
use sha2::{Digest, Sha256};

/// State of a research task.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ResearchStatus {
    Planning,
    AwaitingApproval,
    Denied,
    Executing,
    Synthesizing,
    Completed,
    Failed(String),
}

/// Orchestrator for the Research Workflow.
pub struct ResearchOrchestrator {
    _admin_state: Arc<AdminState>,
    approval_gate: Arc<ChannelApprovalGate>,
    policy: Arc<RwLock<NetworkPolicy>>,
    artifact_store: Arc<dyn ArtifactStore>,
    knowledge_store: Arc<dyn KnowledgeStore>,
    logs_channel: Option<tokio::sync::broadcast::Sender<String>>,
}

impl ResearchOrchestrator {
    pub fn new(
        admin_state: Arc<AdminState>,
        approval_gate: Arc<ChannelApprovalGate>,
        policy: Arc<RwLock<NetworkPolicy>>,
        artifact_store: Arc<dyn ArtifactStore>,
        knowledge_store: Arc<dyn KnowledgeStore>,
        logs_channel: Option<tokio::sync::broadcast::Sender<String>>,
    ) -> Self {
        Self {
            _admin_state: admin_state,
            approval_gate,
            policy,
            artifact_store,
            knowledge_store,
            logs_channel,
        }
    }

    /// Execute the full research workflow.
    pub async fn run_research(&self, session_id: &str, user_id: &str, query: &str) -> Result<String> {
        let trace_id = Uuid::new_v4().to_string();

        self.emit_audit(session_id, &trace_id, EventType::ResearchCreated, serde_json::json!({
            "query": query,
            "orchestrator_version": "P0"
        }));

        // 1. Planning State
        tracing::info!(trace_id, "Transitioning to PLANNING");
        let plan = self.plan_research(session_id, user_id, &trace_id, query).await?;
        
        // 2. Policy Evaluation
        tracing::info!(trace_id, "Transitioning to GOVERNANCE");
        let decision = self.check_policy(&plan).await;
        
        self.emit_audit(session_id, &trace_id, EventType::PolicyEvaluated, serde_json::json!({
            "decision": decision,
            "plan_summary": plan.goals
        }));

        // 3. Approval Gate if needed
        if matches!(decision, NetworkDecision::Denied(_)) || self.requires_approval(&plan, &decision).await {
            tracing::info!(trace_id, "Transitioning to AWAITING_APPROVAL");
            self.emit_audit(session_id, &trace_id, EventType::ApprovalRequested, serde_json::json!({
                "plan": plan,
                "reason": "Policy check required or high risk"
            }));

            let approval_req = multi_agent_core::types::ApprovalRequest {
                request_id: Uuid::new_v4().to_string(),
                session_id: session_id.to_string(),
                tool_name: "research_agent".to_string(),
                args: serde_json::to_value(&plan).unwrap_or_default(),
                risk_level: multi_agent_core::types::ToolRiskLevel::High,
                context: format!("Research query: {}", query),
                timeout_secs: Some(600),
                nonce: Uuid::new_v4().to_string(),
                expires_at: (Utc::now() + chrono::Duration::seconds(600)).timestamp(),
            };

            let response = self.approval_gate.request_approval(&approval_req).await?;
            
            match response {
                multi_agent_core::types::ApprovalResponse::Approved { .. } => {
                    self.emit_audit(session_id, &trace_id, EventType::ApprovalDecided, serde_json::json!({"status": "APPROVED"}));
                }
                _ => {
                    self.emit_audit(session_id, &trace_id, EventType::ApprovalDecided, serde_json::json!({"status": "DENIED"}));
                    return Err(Error::governance("Research task denied by administrator".to_string()));
                }
            }
        }

        // 4. Execution State (Airlock)
        tracing::info!(trace_id, "Transitioning to EXECUTION");
        let findings = self.execute_research(session_id, &trace_id, &plan).await?;
        
        // 5. Synthesis State
        tracing::info!(trace_id, "Transitioning to SYNTHESIS");
        let report = self.synthesize_findings(session_id, user_id, &trace_id, query, findings).await?;
        
        self.emit_audit(session_id, &trace_id, EventType::ReportGenerated, serde_json::json!({
             "report_len": report.len(),
             "status": "COMPLETED"
        }));

        Ok(report)
    }

    async fn plan_research(&self, session_id: &str, user_id: &str, trace_id: &str, query: &str) -> Result<ResearchPlan> {
        self.emit_audit(session_id, trace_id, EventType::PlanProposed, serde_json::json!({"query": query}));
        
        // Use Rig for planning (M10.1)
        let client = openai::Client::from_env();
        let planner = client.agent("gpt-4o")
            .preamble("You are a research planner. Analyze the query and provide a structured research plan including goals, list of domains to visit, and crawl limits. Output MUST be valid JSON.")
            .build();

        let plan = planner.prompt(query).await
            .map_err(|e| Error::internal(format!("Rig planning error: {}", e)))?;

        // Extract structured JSON from the LLM response
        // In a real M10 we'd use rig's structured output but here we demonstrate the intent
        let mut plan: ResearchPlan = serde_json::from_str(&plan)
            .map_err(|e| Error::internal(format!("Failed to parse research plan: {}", e)))?;

        plan.user_id = Some(user_id.to_string());

        Ok(plan)
    }

    async fn check_policy(&self, plan: &ResearchPlan) -> NetworkDecision {
        let p = self.policy.read().await;
        for domain in &plan.candidate_domains {
            // Ensure we handle the Result from check()
            match p.check(domain) {
                Ok(NetworkDecision::Denied(reason)) => return NetworkDecision::Denied(reason),
                Err(e) => return NetworkDecision::Denied(format!("Invalid URL: {}", e)),
                _ => {}
            }
        }
        NetworkDecision::Allowed
    }

    async fn requires_approval(&self, _plan: &ResearchPlan, decision: &NetworkDecision) -> bool {
        // High risk if not explicitly allowed or unknown domains
        matches!(decision, NetworkDecision::Allowed) // For now, assume we want approval for unknown domains
    }

    async fn execute_research(&self, session_id: &str, trace_id: &str, plan: &ResearchPlan) -> Result<Vec<String>> {
        let mut results = Vec::new();
        
        for domain in &plan.candidate_domains {
            let url = format!("https://{}/", domain);
            
            // Emit EGRESS_REQUEST
            self.emit_audit(session_id, trace_id, EventType::EgressRequest, serde_json::json!({
                "url": url,
                "method": "GET"
            }));

            // In a real M10, we'd call the FetchTool here.
            let simulated_content = format!("Content from {}. This is a simulated research finding for query: {}", domain, trace_id);
            
            // M10.2: Persist finding to ArtifactStore
            let ref_id = self.artifact_store.save_with_type(
                bytes::Bytes::from(simulated_content.clone()),
                "text/plain"
            ).await?;

            let mut hasher = Sha256::new();
            hasher.update(simulated_content.as_bytes());
            let body_hash = format!("{:x}", hasher.finalize());

            // Emit EGRESS_RESULT with reference to artifact
            self.emit_audit(session_id, trace_id, EventType::EgressResult, serde_json::json!({
                "url": url,
                "status": 200,
                "body_hash": body_hash,
                "artifact_id": ref_id
            }));

            results.push(simulated_content);
        }
        
        Ok(results)
    }

    async fn synthesize_findings(&self, session_id: &str, user_id: &str, _trace_id: &str, query: &str, findings: Vec<String>) -> Result<String> {
        // M10.5: Synthesis (Rig based)
        let client = openai::Client::from_env();
        let synthesis_agent = client.agent("gpt-4o")
            .preamble("You are a research analyst. Consolidate the provided findings into a comprehensive research report.")
            .build();

        let context = findings.join("\n\n---\n\n");
        let prompt = format!("Research Query: {}\n\nFindings:\n{}", query, context);
        
        let report: String = synthesis_agent.prompt(prompt).await
            .map_err(|e| Error::internal(format!("Synthesis error: {}", e)))?;

        // M10.3: Store in Knowledge Base
        let entry = KnowledgeEntry {
            id: Uuid::new_v4().to_string(),
            summary: report.clone(),
            source_task: query.to_string(),
            user_id: user_id.to_string(),
            session_id: session_id.to_string(),
            embedding: vec![0.0; 1536], // Mock embedding for now, real systems would call an embedding model
            tags: vec!["research".to_string()],
            created_at: Utc::now().timestamp(),
        };

        self.knowledge_store.store(entry).await?;

        Ok(report)
    }

    fn emit_audit(&self, session_id: &str, trace_id: &str, event_type: EventType, payload: serde_json::Value) {
        let envelope = EventEnvelope::new(event_type, payload)
            .with_session(session_id)
            .with_trace(trace_id);
        
        if let Some(tx) = &self.logs_channel {
            if let Ok(json) = serde_json::to_string(&envelope) {
                let _ = tx.send(json);
            }
        }
        tracing::info!(?envelope, "Audit Event");
    }
}
