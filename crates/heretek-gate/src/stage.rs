use std::path::PathBuf;

use heretek_core::{Diagnostic, GateKind, StageStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Staged,
    Worktree(PathBuf),
}

impl Target {
    pub fn describe(&self) -> String {
        match self {
            Self::Staged => "staged".to_string(),
            Self::Worktree(path) => path.display().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GateContext {
    pub repo_root: PathBuf,
    pub target: Target,
    pub baseline: Option<String>,
}

impl GateContext {
    pub fn new(repo_root: impl Into<PathBuf>, target: Target) -> Self {
        Self {
            repo_root: repo_root.into(),
            target,
            baseline: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageOutcome {
    pub status: StageStatus,
    pub diagnostics: Vec<Diagnostic>,
    pub skipped_reason: Option<String>,
}

impl StageOutcome {
    pub fn passed() -> Self {
        Self {
            status: StageStatus::Passed,
            diagnostics: Vec::new(),
            skipped_reason: None,
        }
    }

    pub fn failed(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            status: StageStatus::Failed,
            diagnostics,
            skipped_reason: None,
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            status: StageStatus::Skipped,
            diagnostics: Vec::new(),
            skipped_reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("required tool not found: {tool}")]
    ToolMissing { tool: String },
    #[error("stage failed: {message}")]
    Failed { message: String },
}

pub trait Stage: Send + Sync {
    fn id(&self) -> &'static str;
    fn kind(&self) -> GateKind;
    fn run(&self, ctx: &GateContext) -> Result<StageOutcome, GateError>;
}
