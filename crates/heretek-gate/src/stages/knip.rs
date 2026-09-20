use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use serde_json::Value;

use crate::files;
use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome, Target};
use crate::stages::util;

const GATE: &str = "knip";
const MAX_DIAGNOSTICS: usize = 200;

const CATEGORIES: [(&str, &str); 11] = [
    ("dependencies", "unused dependencies"),
    ("devDependencies", "unused devDependencies"),
    (
        "optionalPeerDependencies",
        "unused optional peer dependencies",
    ),
    ("unlisted", "unlisted dependencies"),
    ("binaries", "unused binaries"),
    ("unresolved", "unresolved imports"),
    ("exports", "unused exports"),
    ("types", "unused types"),
    ("duplicates", "duplicate exports"),
    ("enumMembers", "unused enum members"),
    ("classMembers", "unused class members"),
];

pub struct KnipStage {
    program: Option<PathBuf>,
    timeout: Duration,
    max_output: usize,
    allow_network: bool,
}

impl KnipStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        Self {
            program: files::resolve_tool(repo_root, "knip"),
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
            allow_network: config.allow_network,
        }
    }
}

impl Stage for KnipStage {
    fn id(&self) -> &'static str {
        GATE
    }

    fn kind(&self) -> GateKind {
        GateKind::Advisory
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        let Some(program) = &self.program else {
            return Ok(StageOutcome::skipped("knip not found"));
        };
        if !ctx.files.iter().any(|file| util::is_ts(file)) {
            return Ok(StageOutcome::skipped(
                "no TypeScript or JavaScript files changed",
            ));
        }

        let root = target_root(ctx);
        let spec = ProcessSpec::new(program, &root)
            .args(["--reporter", "json", "--no-progress", "--no-exit-code"])
            .timeout(self.timeout)
            .max_output_bytes(self.max_output)
            .allow_network(self.allow_network);
        let output = run(&spec)?;
        if output.timed_out {
            return Err(GateError::Failed {
                message: format!("knip timed out after {}s", self.timeout.as_secs()),
            });
        }

        let parsed = parse_json(&output.stdout).or_else(|| parse_json(&output.stderr));
        let Some(value) = parsed else {
            if output.success() {
                return Ok(StageOutcome::passed());
            }
            return Err(GateError::Failed {
                message: format!(
                    "knip exited with {:?}: {}",
                    output.status,
                    util::truncate(output.combined().trim(), 600)
                ),
            });
        };

        Ok(StageOutcome::passed_with(collect_diagnostics(&value)))
    }
}

fn target_root(ctx: &GateContext) -> PathBuf {
    match &ctx.target {
        Target::Staged => ctx.repo_root.clone(),
        Target::Worktree(path) => path.clone(),
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

fn collect_diagnostics(value: &Value) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(files) = value.get("files").and_then(Value::as_array) {
        for file in files.iter().filter_map(Value::as_str) {
            push_diagnostic(&mut diagnostics, file, 0, "unused file");
            if diagnostics.len() >= MAX_DIAGNOSTICS {
                return diagnostics;
            }
        }
    }
    if let Some(issues) = value.get("issues").and_then(Value::as_array) {
        for issue in issues {
            let file = issue.get("file").and_then(Value::as_str).unwrap_or("");
            for (key, label) in CATEGORIES {
                let Some(entries) = issue.get(key).and_then(Value::as_array) else {
                    continue;
                };
                let names: Vec<&str> = entries.iter().filter_map(symbol_name).collect();
                if names.is_empty() {
                    continue;
                }
                let line = entries.first().and_then(symbol_line).unwrap_or(0);
                let listed: Vec<&str> = names.iter().take(10).copied().collect();
                let mut message = format!("{label}: {}", listed.join(", "));
                if names.len() > 10 {
                    message.push_str(&format!(" (+{} more)", names.len() - 10));
                }
                push_diagnostic(&mut diagnostics, file, line, &message);
                if diagnostics.len() >= MAX_DIAGNOSTICS {
                    return diagnostics;
                }
            }
        }
    }
    diagnostics
}

fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, file: &str, line: u32, message: &str) {
    let mut diagnostic = util::diagnostic(
        GATE,
        Severity::Note,
        file,
        line,
        0,
        util::truncate(message, 400),
    );
    diagnostic.code = Some(GATE.to_string());
    diagnostics.push(diagnostic);
}

fn symbol_name(entry: &Value) -> Option<&str> {
    entry
        .as_str()
        .or_else(|| entry.get("name").and_then(Value::as_str))
}

fn symbol_line(entry: &Value) -> Option<u32> {
    entry
        .get("line")
        .and_then(Value::as_u64)
        .map(|line| line as u32)
}
