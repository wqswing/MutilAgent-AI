use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::capability::AgentCapability;
use multi_agent_core::{
    types::{AgentResult, Session},
    Error, Result,
};

fn sanitize_text(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(220)
        .collect()
}

fn current_date() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn append_unique_line(path: &Path, header: &str, line: &str) -> Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if !existing.contains(line) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| Error::controller(format!("open writeback file failed: {}", e)))?;

        if existing.is_empty() {
            writeln!(file, "{}", header)
                .map_err(|e| Error::controller(format!("write header failed: {}", e)))?;
        }
        writeln!(file, "{}", line)
            .map_err(|e| Error::controller(format!("append line failed: {}", e)))?;
    }
    Ok(())
}

fn merge_into_memory_md(base_dir: &Path) -> Result<()> {
    let mut merged = HashSet::new();
    let mut ordered = Vec::new();
    let memory_path = base_dir.join("MEMORY.md");

    if let Ok(existing) = std::fs::read_to_string(&memory_path) {
        for line in existing.lines() {
            if line.starts_with("- [") && merged.insert(line.to_string()) {
                ordered.push(line.to_string());
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if path.file_name().and_then(|f| f.to_str()) == Some("MEMORY.md") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines() {
                    if line.starts_with("- [") && merged.insert(line.to_string()) {
                        ordered.push(line.to_string());
                    }
                }
            }
        }
    }

    ordered.sort();
    let mut output = String::from("# MEMORY\n");
    for line in ordered {
        output.push_str(&line);
        output.push('\n');
    }
    std::fs::write(&memory_path, output)
        .map_err(|e| Error::controller(format!("write MEMORY.md failed: {}", e)))?;
    Ok(())
}

pub fn default_memory_dir() -> PathBuf {
    std::env::var("MULTI_AGENT_MEMORY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".memory"))
}

pub fn flush_pre_compaction(session: &Session, estimated_tokens: usize) -> Result<()> {
    let base = default_memory_dir();
    std::fs::create_dir_all(&base)
        .map_err(|e| Error::controller(format!("create memory dir failed: {}", e)))?;
    let today = current_date();
    let daily_path = base.join(format!("{}.md", today));
    let goal = session
        .task_state
        .as_ref()
        .map(|t| t.goal.clone())
        .unwrap_or_default();
    let line = format!(
        "- [{}][session:{}][kind:PRE-COMPACTION] goal={} history_len={} est_tokens={}",
        today,
        session.id,
        sanitize_text(&goal),
        session.history.len(),
        estimated_tokens
    );
    append_unique_line(&daily_path, &format!("# Memory {}", today), &line)?;
    merge_into_memory_md(&base)?;
    Ok(())
}

pub struct MemoryWritebackCapability {
    base_dir: PathBuf,
}

impl MemoryWritebackCapability {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn from_env() -> Self {
        Self {
            base_dir: default_memory_dir(),
        }
    }
}

#[async_trait]
impl AgentCapability for MemoryWritebackCapability {
    fn name(&self) -> &str {
        "memory_writeback"
    }

    async fn on_finish(&self, session: &mut Session, result: &AgentResult) -> Result<()> {
        let goal = session
            .task_state
            .as_ref()
            .map(|t| t.goal.clone())
            .unwrap_or_default();
        if goal.is_empty() {
            return Ok(());
        }

        let result_text = match result {
            AgentResult::Text(s) => s.clone(),
            AgentResult::Data(v) => v.to_string(),
            AgentResult::File { filename, .. } => format!("file: {}", filename),
            AgentResult::UiComponent { component_type, .. } => {
                format!("ui_component: {}", component_type)
            }
            AgentResult::Error { message, .. } => format!("error: {}", message),
        };

        std::fs::create_dir_all(&self.base_dir)
            .map_err(|e| Error::controller(format!("create memory dir failed: {}", e)))?;

        let today = current_date();
        let daily_path = self.base_dir.join(format!("{}.md", today));
        let line = format!(
            "- [{}][session:{}][kind:finish] goal={} result={}",
            today,
            session.id,
            sanitize_text(&goal),
            sanitize_text(&result_text)
        );
        append_unique_line(&daily_path, &format!("# Memory {}", today), &line)?;
        merge_into_memory_md(&self.base_dir)?;
        Ok(())
    }
}
