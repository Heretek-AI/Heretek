use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use serde_json::Value;

use crate::files;
use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome, Target};
use crate::stages::util;

const UNPARSEABLE: &str = "tool output could not be parsed (possibly truncated); increase gate.max_output_bytes or reduce scope";

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
        let files: Vec<String> = changed
            .into_iter()
            .filter(|file| !is_pathological(file))
            .collect();
        if files.is_empty() {
            return Ok(StageOutcome::skipped(
                "only pathological file names changed",
            ));
        }

        let mut args = vec![
            "check".to_string(),
            "--no-errors-on-unmatched".to_string(),
            "--reporter=json".to_string(),
        ];
        if ctx.fix {
            args.insert(1, "--write".to_string());
        }
        args.extend(files);

        let root = ctx.target_root();
        let spec = ProcessSpec::new(program, root)
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
        if output.truncated(self.max_output) {
            return Err(GateError::Failed {
                message: UNPARSEABLE.to_string(),
            });
        }

        let Some(diagnostics) = parse_diagnostics(root, &output.stdout) else {
            if !output.combined().trim().is_empty() {
                return Err(GateError::Failed {
                    message: UNPARSEABLE.to_string(),
                });
            }
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

        if matches!(ctx.target, Target::Staged) || !ctx.fix {
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

fn is_pathological(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with('-') || path.contains('\n')
}

fn as_warning(mut diagnostic: Diagnostic) -> Diagnostic {
    diagnostic.severity = Severity::Warning;
    diagnostic
}

fn parse_diagnostics(root: &Path, stdout: &str) -> Option<Vec<Diagnostic>> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    let entries = value.get("diagnostics")?.as_array()?;
    Some(
        entries
            .iter()
            .map(|entry| biome_diagnostic(root, entry))
            .collect(),
    )
}

fn biome_diagnostic(root: &Path, entry: &Value) -> Diagnostic {
    let file = biome_file(entry);
    let severity = entry
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = biome_message(entry);
    let category = entry
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (line, column) = biome_position(root, entry, &file);

    let mut diagnostic = util::diagnostic(
        "format",
        biome_severity(severity),
        &file,
        line,
        column,
        util::truncate(&message, 800),
    );
    if !category.is_empty() {
        diagnostic.code = Some(category.to_string());
    }
    diagnostic
}

fn biome_file(entry: &Value) -> String {
    let path = entry.pointer("/location/path");
    path.and_then(Value::as_str)
        .or_else(|| {
            path.and_then(|path| path.get("file"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string()
}

fn biome_message(entry: &Value) -> String {
    match entry.get("message") {
        Some(Value::String(message)) => message.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("content").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn biome_position(root: &Path, entry: &Value, file: &str) -> (u32, u32) {
    let start = entry.pointer("/location/start");
    let line = start
        .and_then(|start| start.get("line"))
        .and_then(Value::as_u64);
    let column = start
        .and_then(|start| start.get("column"))
        .and_then(Value::as_u64);
    if let (Some(line), Some(column)) = (line, column) {
        return (line as u32, column as u32);
    }
    let Some(offset) = entry.pointer("/location/span/0").and_then(Value::as_u64) else {
        return (0, 0);
    };
    offset_to_line_column(root, file, offset)
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

fn offset_to_line_column(root: &Path, file: &str, offset: u64) -> (u32, u32) {
    let Some(content) = read_source(root, file) else {
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

fn read_source(root: &Path, file: &str) -> Option<String> {
    if file.is_empty() {
        return None;
    }
    let path = Path::new(file);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    std::fs::read_to_string(candidate).ok()
}
