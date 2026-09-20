use std::path::{Path, PathBuf};
use std::sync::Arc;

use heretek_core::{Diagnostic, GateConfig, GateKind, StageStatus, validate_git_ref};

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
    pub files: Vec<String>,
    pub config: Arc<GateConfig>,
    pub root: PathBuf,
    pub fix: bool,
}

impl GateContext {
    pub fn new(repo_root: impl Into<PathBuf>, target: Target) -> Self {
        let repo_root = repo_root.into();
        let root = match &target {
            Target::Worktree(path) => path.clone(),
            Target::Staged => repo_root.clone(),
        };
        Self {
            repo_root,
            target,
            baseline: None,
            files: Vec::new(),
            config: Arc::new(GateConfig::default()),
            root,
            fix: false,
        }
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }

    pub fn with_config(mut self, config: Arc<GateConfig>) -> Self {
        self.config = config;
        self
    }

    pub fn with_baseline(mut self, baseline: Option<String>) -> Self {
        self.baseline = baseline;
        self
    }

    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = root.into();
        self
    }

    pub fn with_fix(mut self, fix: bool) -> Self {
        self.fix = fix;
        self
    }

    pub fn target_root(&self) -> &Path {
        &self.root
    }

    pub fn validate(&self) -> Result<(), GateError> {
        if let Some(baseline) = &self.baseline {
            validate_git_ref(baseline).map_err(|message| GateError::Failed { message })?;
        }
        Ok(())
    }
}

pub struct PreparedContext {
    pub ctx: GateContext,
    cleanup: Option<PathBuf>,
}

impl PreparedContext {
    fn plain(ctx: GateContext) -> Self {
        Self { ctx, cleanup: None }
    }
}

impl Drop for PreparedContext {
    fn drop(&mut self) {
        if let Some(path) = self.cleanup.take() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

impl GateContext {
    pub fn prepared(&self) -> Result<PreparedContext, GateError> {
        self.validate()?;
        match &self.target {
            Target::Worktree(path) => {
                Ok(PreparedContext::plain(self.clone().with_root(path.clone())))
            }
            Target::Staged => {
                let root = self
                    .repo_root
                    .join(".heretek")
                    .join(format!("staged-{}", std::process::id()));
                if root.exists() {
                    std::fs::remove_dir_all(&root)?;
                }
                std::fs::create_dir_all(&root)?;
                crate::files::git_output(
                    &self.repo_root,
                    &[
                        "checkout-index".to_string(),
                        "--all".to_string(),
                        "--force".to_string(),
                        format!("--prefix={}/", root.display()),
                    ],
                )?;
                #[cfg(unix)]
                {
                    let node_modules = self.repo_root.join("node_modules");
                    if node_modules.exists() {
                        let _ =
                            std::os::unix::fs::symlink(&node_modules, root.join("node_modules"));
                    }
                }
                Ok(PreparedContext {
                    ctx: self.clone().with_root(root.clone()),
                    cleanup: Some(root),
                })
            }
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

    pub fn passed_with(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            status: StageStatus::Passed,
            diagnostics,
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
