use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use serde_json::Value;

use crate::files;
use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome, Target};
use crate::stages::util;

pub struct SecretsStage {
    program: Option<PathBuf>,
    timeout: Duration,
    max_output: usize,
    allow_network: bool,
}

impl SecretsStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        Self {
            program: files::resolve_tool(repo_root, "semgrep"),
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
            allow_network: config.allow_network,
        }
    }
}

impl Stage for SecretsStage {
    fn id(&self) -> &'static str {
        "secrets"
    }

    fn kind(&self) -> GateKind {
        GateKind::Advisory
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        run_semgrep(
            self.program.as_deref(),
            "secrets",
            "p/secrets",
            self.timeout,
            self.max_output,
            self.allow_network,
            ctx,
        )
    }
}

pub struct SastStage {
    program: Option<PathBuf>,
    timeout: Duration,
    max_output: usize,
    allow_network: bool,
}

impl SastStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        Self {
            program: files::resolve_tool(repo_root, "semgrep"),
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
            allow_network: config.allow_network,
        }
    }
}

impl Stage for SastStage {
    fn id(&self) -> &'static str {
        "sast"
    }

    fn kind(&self) -> GateKind {
        GateKind::Advisory
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        run_semgrep(
            self.program.as_deref(),
            "sast",
            "p/default",
            self.timeout,
            self.max_output,
            self.allow_network,
            ctx,
        )
    }
}

fn run_semgrep(
    program: Option<&Path>,
    gate: &str,
    registry_config: &str,
    timeout: Duration,
    max_output: usize,
    allow_network: bool,
    ctx: &GateContext,
) -> Result<StageOutcome, GateError> {
    let Some(program) = program else {
        return Ok(StageOutcome::skipped("semgrep not found"));
    };
    if ctx.files.is_empty() {
        return Ok(StageOutcome::skipped("no changed files"));
    }
    let Some(config) = resolve_config(ctx, registry_config) else {
        return Ok(StageOutcome::skipped(
            "no semgrep config and registry access requires network",
        ));
    };

    let root = target_root(ctx);
    let mut args = vec![
        "scan".to_string(),
        "--json".to_string(),
        "--quiet".to_string(),
        "--metrics=off".to_string(),
        "--disable-version-check".to_string(),
        "--config".to_string(),
        config,
    ];
    args.extend(ctx.files.iter().cloned());

    let spec = ProcessSpec::new(program, &root)
        .args(args)
        .timeout(timeout)
        .max_output_bytes(max_output)
        .allow_network(allow_network);
    let output = run(&spec)?;
    if output.timed_out {
        return Err(GateError::Failed {
            message: format!("{gate} timed out after {}s", timeout.as_secs()),
        });
    }

    let parsed = parse_json(&output.stdout).or_else(|| parse_json(&output.stderr));
    let Some(value) = parsed else {
        if output.success() {
            return Ok(StageOutcome::passed());
        }
        return Err(GateError::Failed {
            message: format!(
                "{gate} exited with {:?}: {}",
                output.status,
                util::truncate(output.combined().trim(), 600)
            ),
        });
    };

    let diagnostics = result_diagnostics(gate, &value);
    if diagnostics.is_empty() {
        return Ok(StageOutcome::passed());
    }
    Ok(StageOutcome::failed(diagnostics))
}

fn target_root(ctx: &GateContext) -> PathBuf {
    match &ctx.target {
        Target::Staged => ctx.repo_root.clone(),
        Target::Worktree(path) => path.clone(),
    }
}

fn resolve_config(ctx: &GateContext, registry_config: &str) -> Option<String> {
    if let Some(config) = &ctx.config.semgrep_config {
        return Some(config.clone());
    }
    for name in ["semgrep.yml", ".semgrep.yml"] {
        let candidate = ctx.repo_root.join(name);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    if ctx.config.allow_network {
        return Some(registry_config.to_string());
    }
    None
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

fn result_diagnostics(gate: &str, value: &Value) -> Vec<Diagnostic> {
    let Some(results) = value.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    results
        .iter()
        .map(|result| {
            let file = result.get("path").and_then(Value::as_str).unwrap_or("");
            let start = result.get("start");
            let line = start
                .and_then(|start| start.get("line"))
                .and_then(Value::as_u64)
                .map_or(0, |value| value as u32);
            let column = start
                .and_then(|start| start.get("col"))
                .and_then(Value::as_u64)
                .map_or(0, |value| value as u32);
            let extra = result.get("extra");
            let message = extra
                .and_then(|extra| extra.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let severity = match extra
                .and_then(|extra| extra.get("severity"))
                .and_then(Value::as_str)
            {
                Some("INFO") => Severity::Note,
                _ => Severity::Warning,
            };
            let mut diagnostic = util::diagnostic(
                gate,
                severity,
                file,
                line,
                column,
                util::truncate(message, 800),
            );
            if let Some(code) = result.get("check_id").and_then(Value::as_str) {
                diagnostic.code = Some(code.to_string());
            }
            diagnostic
        })
        .collect()
}
