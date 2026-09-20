use std::process::Command;

use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct Tool {
    name: &'static str,
    required: bool,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "git",
        required: true,
    },
    Tool {
        name: "cargo",
        required: false,
    },
    Tool {
        name: "node",
        required: false,
    },
    Tool {
        name: "pnpm",
        required: false,
    },
    Tool {
        name: "tsgo",
        required: false,
    },
    Tool {
        name: "biome",
        required: false,
    },
    Tool {
        name: "vitest",
        required: false,
    },
    Tool {
        name: "ast-grep",
        required: false,
    },
    Tool {
        name: "semgrep",
        required: false,
    },
    Tool {
        name: "knip",
        required: false,
    },
    Tool {
        name: "osv-scanner",
        required: false,
    },
    Tool {
        name: "llama-server",
        required: false,
    },
];

pub fn doctor_json() -> Value {
    let tools: Vec<Value> = TOOLS.iter().map(probe).collect();
    json!({ "tools": tools })
}

fn probe(tool: &Tool) -> Value {
    let version = version_of(tool.name);
    json!({
        "name": tool.name,
        "required": tool.required,
        "found": version.is_some(),
        "version": version,
    })
}

fn version_of(tool: &str) -> Option<String> {
    let output = Command::new(tool).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    Some(truncate(line, 64))
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(3)).collect();
    format!("{head}...")
}
