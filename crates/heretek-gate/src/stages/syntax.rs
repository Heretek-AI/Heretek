use std::path::Path;
use std::time::Duration;

use heretek_core::{Diagnostic, GateConfig, GateKind, Severity};
use tree_sitter::{Language, Node, Parser};

use crate::process::{ProcessSpec, run};
use crate::stage::{GateContext, GateError, Stage, StageOutcome, Target};
use crate::stages::util;

const MAX_DIAGNOSTICS_PER_FILE: usize = 20;

pub struct SyntaxStage {
    timeout: Duration,
    max_output: usize,
}

impl SyntaxStage {
    pub fn new(_repo_root: &Path, config: &GateConfig) -> Self {
        Self {
            timeout: Duration::from_secs(config.stage_timeout_secs),
            max_output: config.max_output_bytes,
        }
    }

    fn source(&self, ctx: &GateContext, file: &str) -> Option<String> {
        match &ctx.target {
            Target::Worktree(root) => std::fs::read_to_string(root.join(file)).ok(),
            Target::Staged => {
                let spec = ProcessSpec::new("git", &ctx.repo_root)
                    .args(["show", &format!(":{file}")])
                    .timeout(self.timeout)
                    .max_output_bytes(self.max_output)
                    .allow_network(false);
                let output = run(&spec).ok()?;
                output.success().then_some(output.stdout)
            }
        }
    }
}

impl Stage for SyntaxStage {
    fn id(&self) -> &'static str {
        "syntax"
    }

    fn kind(&self) -> GateKind {
        GateKind::Blocking
    }

    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError> {
        let changed: Vec<&String> = ctx.files.iter().filter(|file| util::is_ts(file)).collect();
        if changed.is_empty() {
            return Ok(StageOutcome::skipped(
                "no TypeScript or JavaScript files changed",
            ));
        }

        let mut parser = Parser::new();
        let mut diagnostics = Vec::new();

        for file in changed {
            let Some(language) = language_for(file) else {
                continue;
            };
            let Some(source) = self.source(ctx, file) else {
                continue;
            };
            if parser.set_language(&language).is_err() {
                continue;
            }
            let Some(tree) = parser.parse(source.as_bytes(), None) else {
                continue;
            };
            diagnostics.extend(collect_diagnostics(file, tree.root_node()));
        }

        if diagnostics.is_empty() {
            Ok(StageOutcome::passed())
        } else {
            Ok(StageOutcome::failed(diagnostics))
        }
    }
}

fn language_for(file: &str) -> Option<Language> {
    let extension = Path::new(file).extension()?.to_str()?;
    match extension {
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "ts" | "mts" | "cts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        _ => None,
    }
}

fn collect_diagnostics(file: &str, root: Node<'_>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if diagnostics.len() >= MAX_DIAGNOSTICS_PER_FILE {
            break;
        }
        if node.is_error() || node.is_missing() {
            diagnostics.push(node_diagnostic(file, &node));
        }
        for index in (0..node.child_count()).rev() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    diagnostics
}

fn node_diagnostic(file: &str, node: &Node<'_>) -> Diagnostic {
    let position = node.start_position();
    let message = if node.is_missing() {
        format!("missing token: {}", node.kind())
    } else {
        format!("syntax error near '{}'", node.kind())
    };
    let mut diagnostic = util::diagnostic(
        "syntax",
        Severity::Error,
        file,
        position.row as u32 + 1,
        position.column as u32 + 1,
        message,
    );
    diagnostic.code = Some("syntax".to_string());
    diagnostic
}
