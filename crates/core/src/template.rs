//! Template Hydration Engine (L-T Layer)
//!
//! Provides dynamic YAML template rendering using Tera templating engine.
//! Enables Config-as-Code pattern for Agent configurations.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::{Context, Tera};

/// Configuration generated from a hydrated template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// The rendered system prompt with all variables injected.
    pub system_prompt: String,
    /// List of MCP server URIs to connect.
    pub mcp_servers: Vec<String>,
    /// List of native tool names to enable.
    pub native_tools: Vec<String>,
    /// Optional SOP workflow definition.
    pub sop_flow: Option<SopDefinition>,
}

/// SOP workflow definition parsed from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopDefinition {
    pub name: String,
    pub steps: Vec<SopStep>,
}

/// A single step in an SOP workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopStep {
    pub id: String,
    pub description: String,
    /// Tools allowed for this step (privilege de-escalation).
    #[serde(default)]
    pub allow_tools: Vec<String>,
    /// Expected output schema (for validation).
    pub output_schema: Option<String>,
}

/// Raw template structure parsed from YAML before hydration.
#[derive(Debug, Deserialize)]
struct RawTemplate {
    #[serde(default)]
    system_prompt: String,
    #[serde(default)]
    mcp_servers: Vec<String>,
    #[serde(default)]
    native_tools: Vec<String>,
    #[serde(default)]
    sop: Option<RawSop>,
}

#[derive(Debug, Deserialize)]
struct RawSop {
    name: String,
    steps: Vec<SopStep>,
}

/// Hydrates a YAML template with the provided variables.
///
/// # Arguments
/// * `yaml_content` - Raw YAML template content (may contain `{{ variable }}` placeholders).
/// * `vars` - HashMap of variable names to values for injection.
///
/// # Returns
/// * `AgentConfig` - Fully rendered and parsed configuration.
///
/// # Example
/// ```ignore
/// let yaml = r#"
/// system_prompt: "Hello, {{ client_name }}!"
/// mcp_servers:
///   - "sqlite://{{ db_path }}"
/// "#;
/// let mut vars = HashMap::new();
/// vars.insert("client_name".to_string(), "Acme Corp".to_string());
/// vars.insert("db_path".to_string(), "/data/acme.db".to_string());
///
/// let config = hydrate_template(yaml, &vars)?;
/// assert_eq!(config.system_prompt, "Hello, Acme Corp!");
/// ```
pub fn hydrate_template(yaml_content: &str, vars: &HashMap<String, String>) -> Result<AgentConfig> {
    // 1. Render the entire YAML through Tera
    let mut tera = Tera::default();
    let mut context = Context::new();
    for (key, value) in vars {
        context.insert(key, value);
    }

    let rendered_yaml = tera
        .render_str(yaml_content, &context)
        .map_err(|e| crate::error::Error::Template(e.to_string()))?;

    // 2. Parse the rendered YAML
    let raw: RawTemplate = serde_yaml::from_str(&rendered_yaml)
        .map_err(|e| crate::error::Error::Template(e.to_string()))?;

    // 3. Assemble the AgentConfig
    let sop_flow = raw.sop.map(|s| SopDefinition {
        name: s.name,
        steps: s.steps,
    });

    Ok(AgentConfig {
        system_prompt: raw.system_prompt,
        mcp_servers: raw.mcp_servers,
        native_tools: raw.native_tools,
        sop_flow,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hydrate_simple_template() {
        let yaml = r#"
system_prompt: "You are an assistant for {{ company }}."
mcp_servers:
  - "sqlite://{{ db }}"
native_tools:
  - "search"
"#;
        let mut vars = HashMap::new();
        vars.insert("company".to_string(), "TestCorp".to_string());
        vars.insert("db".to_string(), "/tmp/test.db".to_string());

        let config = hydrate_template(yaml, &vars).unwrap();

        assert_eq!(config.system_prompt, "You are an assistant for TestCorp.");
        assert_eq!(config.mcp_servers, vec!["sqlite:///tmp/test.db"]);
        assert_eq!(config.native_tools, vec!["search"]);
    }

    #[test]
    fn test_hydrate_with_sop() {
        let yaml = r#"
system_prompt: "Audit agent for {{ client }}"
sop:
  name: "audit_flow"
  steps:
    - id: "step1"
      description: "Fetch data for {{ client }}"
      allow_tools:
        - "db_query"
"#;
        let mut vars = HashMap::new();
        vars.insert("client".to_string(), "Acme".to_string());

        let config = hydrate_template(yaml, &vars).unwrap();

        assert!(config.sop_flow.is_some());
        let sop = config.sop_flow.unwrap();
        assert_eq!(sop.name, "audit_flow");
        assert_eq!(sop.steps[0].description, "Fetch data for Acme");
        assert_eq!(sop.steps[0].allow_tools, vec!["db_query"]);
    }
}
