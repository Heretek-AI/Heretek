use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use serde_json::Value;

use crate::files;
use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome};
use crate::stages::util;

const UNPARSEABLE: &str = "tool output could not be parsed (possibly truncated); increase gate.max_output_bytes or reduce scope";
const MAX_DIAGNOSTICS: usize = 200;

pub struct DepsStage {
    program: Option<PathBuf>,
    timeout: Duration,
    max_output: usize,
}

impl DepsStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        Self {
            program: files::resolve_tool(repo_root, "osv-scanner"),
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
        }
    }
}

impl Stage for DepsStage {
    fn id(&self) -> &'static str {
        "deps"
    }

    fn kind(&self) -> GateKind {
        GateKind::Advisory
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        let Some(program) = &self.program else {
            return Ok(StageOutcome::skipped("osv-scanner not found"));
        };
        if ctx.files.is_empty() {
            return Ok(StageOutcome::skipped("no changed files"));
        }

        let spec = ProcessSpec::new(program, ctx.target_root())
            .args(["scan", "source", "-r", "--format", "json", "."])
            .timeout(self.timeout)
            .max_output_bytes(self.max_output)
            .allow_network(true);
        let output = run(&spec)?;
        if output.timed_out {
            return Err(GateError::Failed {
                message: format!("deps timed out after {}s", self.timeout.as_secs()),
            });
        }

        if output.truncated(self.max_output) {
            return Err(GateError::Failed {
                message: UNPARSEABLE.to_string(),
            });
        }

        let parsed = parse_json(&output.stdout).or_else(|| parse_json(&output.stderr));
        let Some(value) = parsed else {
            if !output.combined().trim().is_empty() {
                return Err(GateError::Failed {
                    message: UNPARSEABLE.to_string(),
                });
            }
            if output.success() {
                return Ok(StageOutcome::passed());
            }
            return Err(GateError::Failed {
                message: format!(
                    "deps exited with {:?}: {}",
                    output.status,
                    util::truncate(output.combined().trim(), 600)
                ),
            });
        };

        let diagnostics = vulnerability_diagnostics(&value);
        if diagnostics.is_empty() {
            return Ok(StageOutcome::passed());
        }
        Ok(StageOutcome::failed(diagnostics))
    }
}

fn parse_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = [trimmed.find('{'), trimmed.find('[')]
        .into_iter()
        .flatten()
        .min()?;
    let mut values = serde_json::Deserializer::from_str(&trimmed[start..]).into_iter::<Value>();
    values.next().and_then(Result::ok)
}

fn vulnerability_diagnostics(value: &Value) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let Some(results) = value.get("results").and_then(Value::as_array) else {
        return diagnostics;
    };
    for result in results {
        let source = result
            .get("source")
            .and_then(|source| source.get("path"))
            .and_then(Value::as_str);
        let Some(packages) = result.get("packages").and_then(Value::as_array) else {
            continue;
        };
        for package in packages {
            let Some(vulnerabilities) = package.get("vulnerabilities").and_then(Value::as_array)
            else {
                continue;
            };
            let info = package.get("package");
            let name = info
                .and_then(|info| info.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let version = info
                .and_then(|info| info.get("version"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let ecosystem = info
                .and_then(|info| info.get("ecosystem"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let file = source.unwrap_or(name);
            for vulnerability in vulnerabilities {
                let id = vulnerability
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("OSV");
                let summary = vulnerability
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let message =
                    util::truncate(&format!("{ecosystem}/{name}@{version}: {summary}"), 400);
                let mut diagnostic =
                    util::diagnostic("deps", Severity::Warning, file, 0, 0, message);
                diagnostic.code = Some(id.to_string());
                diagnostic.hint = Some("upgrade or pin a patched version".to_string());
                diagnostics.push(diagnostic);
                if diagnostics.len() >= MAX_DIAGNOSTICS {
                    return diagnostics;
                }
            }
        }
    }
    diagnostics
}
