use std::collections::BTreeMap;

use heretek_core::{Diagnostic, GateKind, GateReport, StageReport, StageStatus};
use serde::{Deserialize, Serialize};

use crate::pipeline::Pipeline;
use crate::shadow::ShadowWorkspace;
use crate::stage::{GateContext, GateError, Target};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Baseline {
    pub fingerprints: BTreeMap<String, usize>,
}

impl Baseline {
    pub fn from_report(report: &GateReport) -> Self {
        let mut fingerprints = BTreeMap::new();
        for stage in &report.stages {
            for diagnostic in &stage.diagnostics {
                *fingerprints.entry(fingerprint(diagnostic)).or_insert(0) += 1;
            }
        }
        Self { fingerprints }
    }

    pub fn capture(pipeline: &Pipeline, ctx: &GateContext, id: &str) -> Result<Self, GateError> {
        let rev = ctx.baseline.clone().unwrap_or_else(|| "HEAD".to_string());
        let shadow = ShadowWorkspace::create(&ctx.repo_root, &rev, &format!("baseline-{id}"))?;
        let baseline_ctx = GateContext::new(
            ctx.repo_root.clone(),
            Target::Worktree(shadow.path().to_path_buf()),
        )
        .with_config(ctx.config.clone())
        .with_files(ctx.files.clone())
        .with_fix(false);
        let report = pipeline.run(&baseline_ctx);
        Ok(Self::from_report(&report))
    }

    pub fn match_new(&self, report: GateReport) -> GateReport {
        let mut remaining = self.fingerprints.clone();
        let stages: Vec<StageReport> = report
            .stages
            .into_iter()
            .map(|stage| {
                let mut diagnostics = stage.diagnostics;
                for diagnostic in &mut diagnostics {
                    let key = fingerprint(diagnostic);
                    let count = remaining.entry(key).or_insert(0);
                    if *count > 0 {
                        *count -= 1;
                        diagnostic.is_new = false;
                    } else {
                        diagnostic.is_new = true;
                    }
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
    let context = diagnostic
        .context
        .first()
        .map(|line| normalize(line))
        .unwrap_or_default();
    format!(
        "{}|{}|{}|{}|{}",
        diagnostic.gate,
        code,
        diagnostic.file,
        normalize(&diagnostic.message),
        context
    )
}

fn normalize(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}
