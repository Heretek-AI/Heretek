use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use serde_json::Value;

use crate::files;
use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome, Target};
use crate::stages::util;

const GATE: &str = "ast-grep";

pub struct AstGrepStage {
    program: Option<PathBuf>,
    timeout: Duration,
    max_output: usize,
    allow_network: bool,
}

impl AstGrepStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        Self {
            program: files::resolve_tool(repo_root, "ast-grep"),
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
            allow_network: config.allow_network,
        }
    }
}

impl Stage for AstGrepStage {
    fn id(&self) -> &'static str {
        GATE
    }

    fn kind(&self) -> GateKind {
        GateKind::Blocking
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        let Some(program) = &self.program else {
            return Ok(StageOutcome::skipped("ast-grep not found"));
        };
        if ctx.files.is_empty() {
            return Ok(StageOutcome::skipped("no changed files"));
        }
        let Some(rule_args) = rule_args(ctx) else {
            return Ok(StageOutcome::skipped("no ast-grep rules configured"));
        };

        let root = target_root(ctx);
        let mut args = vec!["scan".to_string(), "--json=compact".to_string()];
        args.extend(rule_args);
        args.extend(ctx.files.iter().cloned());

        let spec = ProcessSpec::new(program, &root)
            .args(args)
            .timeout(self.timeout)
            .max_output_bytes(self.max_output)
            .allow_network(self.allow_network);
        let output = run(&spec)?;
        if output.timed_out {
            return Err(GateError::Failed {
                message: format!("ast-grep timed out after {}s", self.timeout.as_secs()),
            });
        }

        let parsed = parse_json(&output.stdout).or_else(|| parse_json(&output.stderr));
        let Some(value) = parsed else {
            if output.success() {
                return Ok(StageOutcome::passed());
            }
            return Err(GateError::Failed {
                message: format!(
                    "ast-grep exited with {:?}: {}",
                    output.status,
                    util::truncate(output.combined().trim(), 600)
                ),
            });
        };

        let diagnostics = match_diagnostics(&value);
        if diagnostics.is_empty() {
            return Ok(StageOutcome::passed());
        }
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return Ok(StageOutcome::failed(diagnostics));
        }
        Ok(StageOutcome::passed_with(diagnostics))
    }
}

fn target_root(ctx: &GateContext) -> PathBuf {
    match &ctx.target {
        Target::Staged => ctx.repo_root.clone(),
        Target::Worktree(path) => path.clone(),
    }
}

fn rule_args(ctx: &GateContext) -> Option<Vec<String>> {
    if let Some(path) = &ctx.config.ast_grep_rules {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            ctx.repo_root.join(path)
        };
        return rule_args_for_path(&path);
    }
    let config = ctx.repo_root.join("sgconfig.yml");
    if config.is_file() {
        return Some(vec!["--config".to_string(), config.display().to_string()]);
    }
    None
}

fn rule_args_for_path(path: &Path) -> Option<Vec<String>> {
    if path.is_file() {
        return Some(vec!["--rule".to_string(), path.display().to_string()]);
    }
    if !path.is_dir() {
        return None;
    }
    let config = path.join("sgconfig.yml");
    if config.is_file() {
        return Some(vec!["--config".to_string(), config.display().to_string()]);
    }
    let mut rules: Vec<PathBuf> = std::fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|entry| {
            entry.is_file()
                && entry
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension == "yml")
        })
        .collect();
    rules.sort();
    rules.truncate(50);
    if rules.is_empty() {
        return None;
    }
    let mut args = Vec::with_capacity(rules.len() * 2);
    for rule in rules {
        args.push("--rule".to_string());
        args.push(rule.display().to_string());
    }
    Some(args)
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

fn match_diagnostics(value: &Value) -> Vec<Diagnostic> {
    let matches = value
        .as_array()
        .or_else(|| value.get("matches").and_then(Value::as_array));
    let Some(matches) = matches else {
        return Vec::new();
    };
    matches.iter().filter_map(match_diagnostic).collect()
}

fn match_diagnostic(entry: &Value) -> Option<Diagnostic> {
    let file = entry
        .get("file")
        .or_else(|| entry.get("path"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let (line, column) = entry
        .get("range")
        .and_then(|range| range.get("start"))
        .map(|start| {
            let line = start
                .get("line")
                .and_then(Value::as_u64)
                .map_or(0, |value| (value as u32) + 1);
            let column = start
                .get("column")
                .and_then(Value::as_u64)
                .map_or(0, |value| (value as u32) + 1);
            (line, column)
        })
        .unwrap_or((0, 0));
    let rule = entry
        .get("ruleId")
        .or_else(|| entry.get("rule_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let message = entry
        .get("message")
        .or_else(|| entry.get("note"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let severity = match entry.get("severity").and_then(Value::as_str) {
        Some("error") => Severity::Error,
        Some("warning") => Severity::Warning,
        _ => Severity::Note,
    };
    let mut diagnostic = util::diagnostic(
        GATE,
        severity,
        file,
        line,
        column,
        util::truncate(message, 800),
    );
    if !rule.is_empty() {
        diagnostic.code = Some(rule.to_string());
    }
    Some(diagnostic)
}
