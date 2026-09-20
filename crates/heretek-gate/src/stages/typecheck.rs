use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};

use crate::files;
use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome};
use crate::stages::util;

pub struct TypecheckStage {
    program: Option<PathBuf>,
    timeout: Duration,
    max_output: usize,
    allow_network: bool,
}

impl TypecheckStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        let program = files::resolve_tool(repo_root, "tsgo")
            .or_else(|| files::resolve_tool(repo_root, "tsc"));
        Self {
            program,
            timeout: Duration::from_secs(config.stage_timeout_secs.min(900)),
            max_output: config.max_output_bytes,
            allow_network: config.allow_network,
        }
    }
}

impl Stage for TypecheckStage {
    fn id(&self) -> &'static str {
        "typecheck"
    }

    fn kind(&self) -> GateKind {
        GateKind::Blocking
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        let Some(program) = &self.program else {
            return Ok(StageOutcome::skipped("tsgo and tsc are not available"));
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

        let project = ctx.target_root().join("tsconfig.json");
        let spec = if project.is_file() {
            ProcessSpec::new(program, ctx.target_root()).args([
                "--noEmit",
                "--pretty",
                "false",
                "-p",
                "tsconfig.json",
            ])
        } else {
            let mut args = vec![
                "--noEmit".to_string(),
                "--pretty".to_string(),
                "false".to_string(),
            ];
            args.extend(changed.iter().cloned());
            ProcessSpec::new(program, ctx.target_root()).args(args)
        }
        .timeout(self.timeout)
        .max_output_bytes(self.max_output)
        .allow_network(self.allow_network);

        let output = run(&spec)?;
        if output.timed_out {
            return Err(GateError::Failed {
                message: format!("typecheck timed out after {}s", self.timeout.as_secs()),
            });
        }

        let diagnostics: Vec<Diagnostic> = output
            .combined()
            .lines()
            .filter_map(|line| parse_diagnostic("typecheck", line))
            .collect();

        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return Ok(StageOutcome::failed(diagnostics));
        }

        if !output.success() {
            let message = util::truncate(output.combined().trim(), 600);
            return Ok(StageOutcome::failed(vec![util::diagnostic(
                "typecheck",
                Severity::Error,
                "",
                0,
                0,
                format!("typecheck exited with {:?}: {message}", output.status),
            )]));
        }

        Ok(StageOutcome::passed_with(diagnostics))
    }
}

fn parse_diagnostic(gate: &str, line: &str) -> Option<Diagnostic> {
    let open = line.find('(')?;
    let close = line[open..].find(')')? + open;
    let file = line[..open].trim();
    if file.is_empty() || !util::is_ts(file) {
        return None;
    }
    let (line_no, column) = util::parse_line_column(&line[open + 1..close]);
    if line_no == 0 {
        return None;
    }
    let rest = line[close + 1..].trim_start_matches(':').trim();
    let (severity_token, after_severity) = rest.split_once(' ')?;
    let severity = match severity_token {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        _ => return None,
    };
    let (code, message) = after_severity
        .split_once(':')
        .map(|(code, message)| (code.trim().to_string(), message.trim().to_string()))
        .unwrap_or_else(|| (String::new(), after_severity.trim().to_string()));
    let mut diagnostic = util::diagnostic(
        gate,
        severity,
        file,
        line_no,
        column,
        util::truncate(&message, 800),
    );
    if !code.is_empty() {
        diagnostic.code = Some(code);
    }
    Some(diagnostic)
}
