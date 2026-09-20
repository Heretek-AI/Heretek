use std::collections::BTreeSet;

use heretek_core::{Diagnostic, GateKind, GateReport, StageReport, StageStatus};
use serde::{Deserialize, Serialize};

use crate::pipeline::Pipeline;
use crate::shadow::ShadowWorkspace;
use crate::stage::{GateContext, GateError, Target};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Baseline {
    pub fingerprints: BTreeSet<String>,
}

impl Baseline {
    pub fn from_report(report: &GateReport) -> Self {
        let mut fingerprints = BTreeSet::new();
        for stage in &report.stages {
            for diagnostic in &stage.diagnostics {
                fingerprints.insert(fingerprint(diagnostic));
            }
        }
        Self { fingerprints }
    }

    pub fn capture(pipeline: &Pipeline, ctx: &GateContext, id: &str) -> Result<Self, GateError> {
        let shadow = ShadowWorkspace::create(&ctx.repo_root, &format!("baseline-{id}"))?;
        let baseline_ctx = GateContext {
            repo_root: ctx.repo_root.clone(),
            target: Target::Worktree(shadow.path().to_path_buf()),
            baseline: None,
            files: ctx.files.clone(),
            config: ctx.config.clone(),
        };
        let report = pipeline.run(&baseline_ctx);
        shadow.cleanup();
        Ok(Self::from_report(&report))
    }

    pub fn is_new(&self, diagnostic: &Diagnostic) -> bool {
        !self.fingerprints.contains(&fingerprint(diagnostic))
    }

    pub fn apply(&self, report: GateReport) -> GateReport {
        let stages: Vec<StageReport> = report
            .stages
            .into_iter()
            .map(|stage| {
                let mut diagnostics = stage.diagnostics;
                for diagnostic in &mut diagnostics {
                    diagnostic.is_new = self.is_new(diagnostic);
                }
                let has_new = diagnostics.iter().any(|diagnostic| diagnostic.is_new);
                let status = if stage.kind == GateKind::Blocking
                    && stage.status == StageStatus::Failed
                    && !has_new
                {
                    StageStatus::Passed
                } else {
                    stage.status
                };
                StageReport {
                    status,
                    diagnostics,
                    ..stage
                }
            })
            .collect();
        let mut updated = GateReport::from_stages(report.target, stages);
        updated.warnings = report.warnings;
        updated
    }
}

pub fn fingerprint(diagnostic: &Diagnostic) -> String {
    let code = diagnostic.code.clone().unwrap_or_default();
    format!(
        "{}|{}|{}|{}",
        diagnostic.gate,
        code,
        diagnostic.file,
        normalize(&diagnostic.message)
    )
}

fn normalize(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}
