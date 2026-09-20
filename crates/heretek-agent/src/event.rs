use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    SessionStart {
        task: String,
        lane: String,
        model: String,
    },
    TurnStart {
        turn: u32,
        max_turns: u32,
        zone1_hash: u64,
        zone2_hash: u64,
    },
    ModelResponse {
        content_chars: usize,
        tool_calls: usize,
        prompt_tokens: u64,
        completion_tokens: u64,
        cached_tokens: Option<u64>,
    },
    Repair {
        pass: String,
        detail: String,
    },
    ToolCall {
        name: String,
        arguments_chars: usize,
    },
    ToolResult {
        name: String,
        is_error: bool,
        chars: usize,
    },
    StormBreak {
        name: String,
    },
    GateRun {
        passed: bool,
        new_diagnostics: usize,
        blocking_failures: usize,
        warnings: Vec<String>,
    },
    GateFeedback {
        frame: String,
    },
    Escalation {
        from: String,
        to: String,
        reason: String,
    },
    Finished {
        summary: String,
        passed: bool,
        turns: u32,
    },
    ConfigReverted {
        paths: Vec<String>,
    },
    AuditObjection {
        frame: String,
    },
    AuditCleared,
    AuditUnavailable {
        reason: String,
    },
    Error {
        message: String,
    },
}

pub struct EventWriter {
    path: PathBuf,
    file: Option<std::fs::File>,
    first_error: Option<String>,
}

impl EventWriter {
    pub fn create(dir: &Path, id: &str) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{id}.jsonl"));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            first_error: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.first_error.take()
    }

    pub fn write(&mut self, event: &Event) {
        use std::io::Write;
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let line = match serde_json::to_string(event) {
            Ok(line) => line,
            Err(error) => {
                self.first_error.get_or_insert_with(|| error.to_string());
                return;
            }
        };
        if let Err(error) = writeln!(file, "{line}")
            && self.first_error.is_none()
        {
            self.first_error = Some(error.to_string());
        }
    }
}
