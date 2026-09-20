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
    Error {
        message: String,
    },
}

pub struct EventWriter {
    path: PathBuf,
    file: Option<std::fs::File>,
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
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&mut self, event: &Event) {
        use std::io::Write;
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if let Ok(line) = serde_json::to_string(event) {
            let _ = writeln!(file, "{line}");
        }
    }
}
