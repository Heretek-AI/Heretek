use std::process::{Command, ExitCode};

use heretek_core::HereConfig;
use heretek_model::Router;

use crate::DoctorArgs;

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

pub fn run(args: &DoctorArgs) -> ExitCode {
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

    let isolation = heretek_gate::process::network_isolation_available();
    let isolation_label = if isolation {
        "available (unshare --user --net)"
    } else {
        "unavailable (best-effort proxy fallback)"
    };
    println!("  {:<14} {:<9} {isolation_label}", "isolation", "network");

    let repo_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("heretek: cannot resolve working directory: {error}");
            return ExitCode::from(3);
        }
    };

    match HereConfig::load(&repo_root) {
        Ok(config) if config.models.is_empty() => {
            println!("  {:<14} {:<9} none configured", "models", "optional");
        }
        Ok(config) => {
            let count = config.models.len();
            if args.models {
                let router = Router::new(config);
                for (name, health) in router.probe() {
                    if health.reachable {
                        println!(
                            "  {:<14} {:<9} reachable in {} ms, {} model(s)",
                            name,
                            "model",
                            health.latency_ms,
                            health.models.len()
                        );
                    } else {
                        let error = health.error.unwrap_or_else(|| "unreachable".to_string());
                        println!("  {:<14} {:<9} {error}", name, "model");
                    }
                }
            } else {
                println!(
                    "  {:<14} {:<9} {count} configured (use --models to probe)",
                    "models", "optional"
                );
            }
        }
        Err(error) => {
            eprintln!("heretek: {error}");
            return ExitCode::from(2);
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
