use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::{BTreeMap, HashSet};
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

fn sqlite_memory_path() -> Option<PathBuf> {
    std::env::var("MULTI_AGENT_MEMORY_SQLITE_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

fn init_sqlite(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS memory_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            session_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            line TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_memory_records_date ON memory_records(date);
    "#,
    )
    .map_err(|e| Error::controller(format!("init memory sqlite failed: {}", e)))?;
    Ok(())
}

fn append_sqlite_record(
    db_path: &Path,
    date: &str,
    session_id: &str,
    kind: &str,
    line: &str,
) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::controller(format!("create memory sqlite dir failed: {}", e)))?;
    }
    let conn = Connection::open(db_path)
        .map_err(|e| Error::controller(format!("open memory sqlite failed: {}", e)))?;
    init_sqlite(&conn)?;
    conn.execute(
        "INSERT OR IGNORE INTO memory_records (date, session_id, kind, line, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![date, session_id, kind, line, Utc::now().timestamp()],
    )
    .map_err(|e| Error::controller(format!("insert memory sqlite failed: {}", e)))?;
    Ok(())
}

fn project_markdown_from_sqlite(db_path: &Path, base_dir: &Path) -> Result<()> {
    let conn = Connection::open(db_path)
        .map_err(|e| Error::controller(format!("open memory sqlite failed: {}", e)))?;
    init_sqlite(&conn)?;

    let mut stmt = conn
        .prepare("SELECT date, line FROM memory_records ORDER BY date ASC, line ASC")
        .map_err(|e| Error::controller(format!("prepare memory sqlite query failed: {}", e)))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| Error::controller(format!("query memory sqlite failed: {}", e)))?;

    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut merged = Vec::new();
    for row in rows {
        let (date, line) =
            row.map_err(|e| Error::controller(format!("scan memory sqlite row failed: {}", e)))?;
        grouped.entry(date).or_default().push(line.clone());
        merged.push(line);
    }

    std::fs::create_dir_all(base_dir)
        .map_err(|e| Error::controller(format!("create memory dir failed: {}", e)))?;
    if let Ok(entries) = std::fs::read_dir(base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md")
                && path.file_name().and_then(|f| f.to_str()) != Some("MEMORY.md")
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    for (date, lines) in grouped {
        let daily_path = base_dir.join(format!("{}.md", date));
        let mut output = format!("# Memory {}\n", date);
        for line in lines {
            output.push_str(&line);
            output.push('\n');
        }
        std::fs::write(&daily_path, output)
            .map_err(|e| Error::controller(format!("write daily projection failed: {}", e)))?;
    }

    let mut memory_out = String::from("# MEMORY\n");
    for line in merged {
        memory_out.push_str(&line);
        memory_out.push('\n');
    }
    std::fs::write(base_dir.join("MEMORY.md"), memory_out)
        .map_err(|e| Error::controller(format!("write MEMORY.md failed: {}", e)))?;
    Ok(())
}

fn append_memory_record(
    base_dir: &Path,
    date: &str,
    session_id: &str,
    kind: &str,
    line: &str,
) -> Result<()> {
    if let Some(db_path) = sqlite_memory_path() {
        append_sqlite_record(&db_path, date, session_id, kind, line)?;
        project_markdown_from_sqlite(&db_path, base_dir)?;
        return Ok(());
    }
    let daily_path = base_dir.join(format!("{}.md", date));
    append_unique_line(&daily_path, &format!("# Memory {}", date), line)?;
    merge_into_memory_md(base_dir)?;
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
    append_memory_record(&base, &today, &session.id, "PRE-COMPACTION", &line)?;
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
        let line = format!(
            "- [{}][session:{}][kind:finish] goal={} result={}",
            today,
            session.id,
            sanitize_text(&goal),
            sanitize_text(&result_text)
        );
        append_memory_record(&self.base_dir, &today, &session.id, "finish", &line)?;
        Ok(())
    }
}
