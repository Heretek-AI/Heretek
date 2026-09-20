use std::path::{Path, PathBuf};
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use serde_json::Value;

use crate::files;
use crate::process::{ProcessOutput, ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome, Target};
use crate::stages::util;

const UNPARSEABLE: &str = "tool output could not be parsed (possibly truncated); increase gate.max_output_bytes or reduce scope";

#[derive(Clone, Copy)]
enum Runner {
    Vitest,
    Jest,
}

pub struct TestsStage {
    program: Option<PathBuf>,
    runner: Runner,
    timeout: Duration,
    max_output: usize,
    allow_network: bool,
}

impl TestsStage {
    pub fn new(repo_root: &Path, config: &GateConfig) -> Self {
        let (program, runner) = if let Some(program) = files::resolve_tool(repo_root, "vitest") {
            (Some(program), Runner::Vitest)
        } else if let Some(program) = files::resolve_tool(repo_root, "jest") {
            (Some(program), Runner::Jest)
        } else {
            (None, Runner::Vitest)
        };
        Self {
            program,
            runner,
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
            allow_network: config.allow_network,
        }
    }

    fn args(&self, filters: &[String], changed_mode: bool) -> Vec<String> {
        let mut args = match self.runner {
            Runner::Vitest => vec![
                "run".to_string(),
                "--reporter=json".to_string(),
                "--no-color".to_string(),
            ],
            Runner::Jest => vec!["--json".to_string(), "--ci".to_string()],
        };
        if changed_mode {
            match self.runner {
                Runner::Vitest => {
                    args.push("--changed".to_string());
                    args.push("HEAD".to_string());
                }
                Runner::Jest => args.push("--onlyChanged".to_string()),
            }
        } else {
            args.extend(filters.iter().cloned());
        }
        args
    }

    fn execute(
        &self,
        program: &Path,
        root: &Path,
        args: Vec<String>,
    ) -> Result<ProcessOutput, GateError> {
        let spec = ProcessSpec::new(program, root)
            .args(args)
            .timeout(self.timeout)
            .max_output_bytes(self.max_output)
            .allow_network(self.allow_network);
        let output = run(&spec)?;
        if output.timed_out {
            return Err(GateError::Failed {
                message: format!("tests timed out after {}s", self.timeout.as_secs()),
            });
        }
        Ok(output)
    }
}

impl Stage for TestsStage {
    fn id(&self) -> &'static str {
        "tests"
    }

    fn kind(&self) -> GateKind {
        GateKind::Blocking
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        let Some(program) = &self.program else {
            return Ok(StageOutcome::skipped("vitest and jest not found"));
        };

        let relevant: Vec<String> = ctx
            .files
            .iter()
            .filter(|file| util::is_ts(file))
            .cloned()
            .collect();
        if relevant.is_empty() {
            return Ok(StageOutcome::skipped("no test files changed"));
        }

        let path_filters: Vec<String> = relevant
            .iter()
            .filter(|file| is_test_file(file))
            .cloned()
            .collect();
        let filters: Vec<String> = path_filters
            .iter()
            .filter(|file| !is_pathological(file))
            .cloned()
            .collect();
        if !path_filters.is_empty() && filters.is_empty() {
            return Ok(StageOutcome::skipped(
                "only pathological file names changed",
            ));
        }
        let root = ctx.target_root();
        let changed_mode = ctx.baseline.is_some() || matches!(ctx.target, Target::Staged);

        let mut output = self.execute(program, root, self.args(&filters, changed_mode))?;
        if output.truncated(self.max_output) {
            return Err(GateError::Failed {
                message: UNPARSEABLE.to_string(),
            });
        }
        let mut parsed = tool_json(&output);

        if changed_mode && !output.success() && !has_test_results(parsed.as_ref()) {
            output = self.execute(program, root, self.args(&filters, false))?;
            if output.truncated(self.max_output) {
                return Err(GateError::Failed {
                    message: UNPARSEABLE.to_string(),
                });
            }
            parsed = tool_json(&output);
        }

        if parsed.is_none() && !output.combined().trim().is_empty() {
            return Err(GateError::Failed {
                message: UNPARSEABLE.to_string(),
            });
        }

        let diagnostics = parsed
            .as_ref()
            .map(assertion_diagnostics)
            .unwrap_or_default();

        if output.success() {
            return Ok(StageOutcome::passed_with(diagnostics));
        }

        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return Ok(StageOutcome::failed(diagnostics));
        }

        Err(GateError::Failed {
            message: format!(
                "tests exited with {:?}: {}",
                output.status,
                util::truncate(output.combined().trim(), 600)
            ),
        })
    }
}

fn is_pathological(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with('-') || path.contains('\n')
}

fn is_test_file(path: &str) -> bool {
    path.contains(".test.")
        || path.contains(".spec.")
        || path.contains("__tests__/")
        || path.contains("/tests/")
        || path.starts_with("tests/")
}

fn tool_json(output: &ProcessOutput) -> Option<Value> {
    parse_json(&output.stdout).or_else(|| parse_json(&output.stderr))
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

fn has_test_results(value: Option<&Value>) -> bool {
    value
        .and_then(|value| value.get("testResults"))
        .and_then(Value::as_array)
        .is_some_and(|results| !results.is_empty())
}

fn assertion_diagnostics(value: &Value) -> Vec<Diagnostic> {
    let Some(results) = value.get("testResults").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    for result in results {
        let file = result.get("name").and_then(Value::as_str).unwrap_or("");
        let mut emitted = false;
        if let Some(assertions) = result.get("assertionResults").and_then(Value::as_array) {
            for assertion in assertions {
                if assertion.get("status").and_then(Value::as_str) != Some("failed") {
                    continue;
                }
                let title = assertion
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("test failed");
                let failure = assertion
                    .get("failureMessages")
                    .and_then(Value::as_array)
                    .and_then(|messages| messages.first())
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let (line, column) = location_from(failure);
                let mut diagnostic = util::diagnostic(
                    "tests",
                    Severity::Error,
                    file,
                    line,
                    column,
                    failure_message(title, failure),
                );
                diagnostic.code = Some("test".to_string());
                diagnostics.push(diagnostic);
                emitted = true;
            }
        }
        if !emitted && result.get("status").and_then(Value::as_str) == Some("failed") {
            let message = result.get("message").and_then(Value::as_str).unwrap_or("");
            let (line, column) = location_from(message);
            let mut diagnostic = util::diagnostic(
                "tests",
                Severity::Error,
                file,
                line,
                column,
                util::truncate(message, 800),
            );
            diagnostic.code = Some("test".to_string());
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

fn failure_message(title: &str, failure: &str) -> String {
    let failure = util::truncate(failure, 800);
    if failure.is_empty() {
        return title.to_string();
    }
    if title.is_empty() {
        return failure;
    }
    format!("{title}: {failure}")
}

fn location_from(message: &str) -> (u32, u32) {
    for (index, _) in message.match_indices(':') {
        let rest = &message[index + 1..];
        let line_digits = rest
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .count();
        if line_digits == 0 {
            continue;
        }
        let after_line = &rest[line_digits..];
        let Some(after_colon) = after_line.strip_prefix(':') else {
            continue;
        };
        let column_digits = after_colon
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .count();
        if column_digits == 0 {
            continue;
        }
        return util::parse_line_column(&format!(
            "{}:{}",
            &rest[..line_digits],
            &after_colon[..column_digits]
        ));
    }
    (0, 0)
}
