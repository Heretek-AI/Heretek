use std::process::{Command, ExitCode};

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

pub fn run() -> ExitCode {
    println!("heretek doctor");
    let mut missing_required = false;

    for tool in TOOLS {
        let label = if tool.required {
            "required"
        } else {
            "optional"
        };
        match version_of(tool.name) {
            Some(version) => println!("  {:<14} {label:<9} {version}", tool.name),
            None => {
                let status = if tool.required {
                    "missing"
                } else {
                    "not found"
                };
                println!("  {:<14} {label:<9} {status}", tool.name);
                missing_required |= tool.required;
            }
        }
    }

    if missing_required {
        eprintln!("heretek: required tooling is missing");
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
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
