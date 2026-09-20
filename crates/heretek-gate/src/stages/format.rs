use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use serde_json::Value;

use crate::files;
use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome, Target};
use crate::stages::util;

pub struct FormatStage {
    program: Option<PathBuf>,
    timeout: Duration,
    max_output: usize,
}

impl FormatStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        Self {
            program: files::resolve_tool(repo_root, "biome"),
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
        }
    }
}

impl Stage for FormatStage {
    fn id(&self) -> &'static str {
        "format"
    }

    fn kind(&self) -> GateKind {
        GateKind::Blocking
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        let Some(program) = &self.program else {
            return Ok(StageOutcome::skipped("biome not found"));
        };

        let changed: Vec<String> = ctx
            .files
            .iter()
            .filter(|file| util::is_ts(file))
            .cloned()
            .collect();
        if changed.is_empty() {
            return Ok(StageOutcome::skipped(
                "no TypeScript or JavaScript files changed",
            ));
        }

        let mut args = vec![
            "check".to_string(),
            "--no-errors-on-unmatched".to_string(),
            "--reporter=json".to_string(),
        ];
        let (cwd, staged) = match &ctx.target {
            Target::Worktree(path) => {
                args.insert(1, "--write".to_string());
                (path.clone(), false)
            }
            Target::Staged => (ctx.repo_root.clone(), true),
        };
        args.extend(changed.iter().cloned());

        let spec = ProcessSpec::new(program, &cwd)
            .args(args)
            .timeout(self.timeout)
            .max_output_bytes(self.max_output)
            .allow_network(false);

        let output = run(&spec)?;
        if output.timed_out {
            return Err(GateError::Failed {
                message: format!("biome check timed out after {}s", self.timeout.as_secs()),
            });
        }

        let Some(diagnostics) = parse_diagnostics(&cwd, &ctx.repo_root, &output.stdout) else {
            if output.success() {
                return Ok(StageOutcome::passed());
            }
            let message = util::truncate(output.combined().trim(), 600);
            return Ok(StageOutcome::failed(vec![util::diagnostic(
                "format",
                Severity::Error,
                "",
                0,
                0,
                format!("biome check exited with {:?}: {message}", output.status),
            )]));
        };

        if staged {
            let diagnostics = diagnostics.into_iter().map(as_warning).collect();
            return Ok(StageOutcome::passed_with(diagnostics));
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

fn as_warning(mut diagnostic: Diagnostic) -> Diagnostic {
    diagnostic.severity = Severity::Warning;
    diagnostic
}

fn parse_diagnostics(root: &Path, repo_root: &Path, stdout: &str) -> Option<Vec<Diagnostic>> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    let entries = value.get("diagnostics")?.as_array()?;
    Some(
        entries
            .iter()
            .map(|entry| biome_diagnostic(root, repo_root, entry))
            .collect(),
    )
}

fn biome_diagnostic(root: &Path, repo_root: &Path, entry: &Value) -> Diagnostic {
    let file = entry
        .pointer("/location/path/file")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let offset = entry
        .pointer("/location/span/0")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let severity = entry
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = entry
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let category = entry
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (line, column) = offset_to_line_column(root, repo_root, file, offset);

    let mut diagnostic = util::diagnostic(
        "format",
        biome_severity(severity),
        file,
        line,
        column,
        util::truncate(message, 800),
    );
    if !category.is_empty() {
        diagnostic.code = Some(category.to_string());
    }
    diagnostic
}

fn biome_severity(value: &str) -> Severity {
    if value.eq_ignore_ascii_case("error") || value.eq_ignore_ascii_case("fatal") {
        Severity::Error
    } else if value.eq_ignore_ascii_case("warning") {
        Severity::Warning
    } else {
        Severity::Note
    }
}

fn offset_to_line_column(root: &Path, repo_root: &Path, file: &str, offset: u64) -> (u32, u32) {
    let Some(content) = read_source(root, repo_root, file) else {
        return (0, 0);
    };
    let bytes = content.as_bytes();
    let end = (offset as usize).min(bytes.len());
    let prefix = String::from_utf8_lossy(&bytes[..end]);
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit('\n')
        .next()
        .map_or(0, |tail| tail.chars().count()) as u32
        + 1;
    (line, column)
}

fn read_source(root: &Path, repo_root: &Path, file: &str) -> Option<String> {
    if file.is_empty() {
        return None;
    }
    let path = Path::new(file);
    let candidates: Vec<PathBuf> = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else if root == repo_root {
        vec![root.join(path)]
    } else {
        vec![root.join(path), repo_root.join(path)]
    };
    candidates
        .into_iter()
        .find_map(|candidate| std::fs::read_to_string(candidate).ok())
}
