//! SOP (Standard Operating Procedure) engine.
//!
//! Supports loading YAML-defined workflows and executing them
//! in order or in parallel using the DAG executor.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use multi_agent_core::{
    traits::{SopDefinition, SopEngine, SopStep, ToolRegistry},
    types::AgentResult,
    Error, Result,
};

/// YAML SOP definition for parsing.
#[derive(Debug, Deserialize, Serialize)]
pub struct YamlSopDefinition {
    /// SOP name.
    pub name: String,
    /// SOP description.
    pub description: Option<String>,
    /// Allow parallel execution.
    #[serde(default)]
    pub allow_parallel: bool,
    /// Steps in the SOP.
    pub steps: Vec<YamlSopStep>,
}

/// YAML SOP step for parsing.
#[derive(Debug, Deserialize, Serialize)]
pub struct YamlSopStep {
    /// Step name.
    pub name: String,
    /// Tool to execute.
    pub tool: String,
    /// Arguments for the tool.
    pub args: serde_json::Value,
    /// Dependencies on other steps.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Tools allowed for this step (privilege de-escalation).
    #[serde(default)]
    pub allow_tools: Vec<String>,
}

/// Default SOP engine implementation.
pub struct DefaultSopEngine {
    /// Tool registry for executing steps.
    tools: Option<std::sync::Arc<dyn ToolRegistry>>,
}

impl DefaultSopEngine {
    /// Create a new SOP engine.
    pub fn new() -> Self {
        Self { tools: None }
    }

    /// Set the tool registry.
    pub fn with_tools(mut self, tools: std::sync::Arc<dyn ToolRegistry>) -> Self {
        self.tools = Some(tools);
        self
    }
}

impl Default for DefaultSopEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A wrapper for SOP Step that implements DagTask
struct SopTask {
    step: SopStep,
    tools: std::sync::Arc<dyn ToolRegistry>,
}

#[async_trait]
impl crate::dag::DagTask for SopTask {
    fn name(&self) -> &str {
        &self.step.name
    }

    fn dependencies(&self) -> &[String] {
        &self.step.depends_on
    }

    async fn execute(&self, context: &HashMap<String, String>) -> Result<String> {
        // Privilege De-escalation: Check if tool is in allowlist
        if !self.step.allow_tools.is_empty() && !self.step.allow_tools.contains(&self.step.tool) {
            return Err(Error::SopExecution(format!(
                "Tool '{}' is not allowed in step '{}'. Allowed tools: {:?}",
                self.step.tool, self.step.name, self.step.allow_tools
            )));
        }
        
        let mut args = self.step.args.clone();
        
        // Inject context and previous results
        if let serde_json::Value::Object(ref mut map) = args {
             // Convert context HashMap to Value
             let ctx_json = serde_json::to_value(context)
                 .map_err(|e| Error::SopExecution(format!("Context serialization error: {}", e)))?;
                 
             map.insert("_previous_results".to_string(), ctx_json);
        }

        let output = self.tools.execute(&self.step.tool, args).await
            .map_err(|e| Error::SopExecution(format!("Step '{}' failed: {}", self.step.name, e)))?;
            
        Ok(output.content)
    }
}

#[async_trait]
impl SopEngine for DefaultSopEngine {
    async fn load(&self, yaml: &str) -> Result<SopDefinition> {
        let parsed: YamlSopDefinition = serde_yaml::from_str(yaml)
            .map_err(|e| Error::SopExecution(format!("Failed to parse SOP YAML: {}", e)))?;

        Ok(SopDefinition {
            name: parsed.name,
            allow_parallel: parsed.allow_parallel,
            steps: parsed
                .steps
                .into_iter()
                .map(|s| SopStep {
                    name: s.name,
                    tool: s.tool,
                    args: s.args,
                    depends_on: s.depends_on,
                    allow_tools: s.allow_tools,
                })
                .collect(),
        })
    }

    async fn execute(&self, sop: &SopDefinition, _context: serde_json::Value) -> Result<AgentResult> {
        tracing::info!(sop = %sop.name, parallel = sop.allow_parallel, "Executing SOP");

        let tools = self.tools.as_ref().ok_or_else(|| {
            Error::SopExecution("No tool registry configured for SOP engine".to_string())
        })?.clone();

        let tasks: Vec<SopTask> = sop.steps.iter().map(|step| SopTask {
            step: step.clone(),
            tools: tools.clone(),
        }).collect();

        let executor = crate::dag::DagExecutor::new(sop.allow_parallel);
        let results = executor.execute(tasks).await?;

        Ok(AgentResult::Data(serde_json::json!({
            "sop": sop.name,
            "results": results
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
        name: test_sop
        allow_parallel: true
        steps:
          - name: step1
            tool: tool1
            args: {}
            depends_on: []
          - name: step2
            tool: tool2
            args: {}
            depends_on: [step1]
        "#;

        let parsed: YamlSopDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.name, "test_sop");
        assert!(parsed.allow_parallel);
        assert_eq!(parsed.steps.len(), 2);
    }
}
