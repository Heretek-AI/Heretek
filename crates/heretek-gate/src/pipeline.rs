use std::time::Instant;

use heretek_core::{Diagnostic, GateKind, GateReport, Severity, StageReport, StageStatus};

use crate::stage::{GateContext, GateError, Stage, StageOutcome};

pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        Self { stages }
    }

    pub fn empty() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn stage_ids(&self) -> Vec<&'static str> {
        self.stages.iter().map(|stage| stage.id()).collect()
    }

    pub fn stage_kinds(&self) -> Vec<(&'static str, GateKind)> {
        self.stages
            .iter()
            .map(|stage| (stage.id(), stage.kind()))
            .collect()
    }

    pub fn run(&self, ctx: &GateContext) -> GateReport {
        let prepared = match ctx.prepared() {
            Ok(prepared) => prepared,
            Err(error) => return setup_failure(ctx, error),
        };
        let ctx = &prepared.ctx;

        let mut reports = Vec::with_capacity(self.stages.len());
        for stage in &self.stages {
            let started = Instant::now();
            let report = match stage.run(ctx) {
                Ok(outcome) => to_report(stage.as_ref(), elapsed_ms(started), outcome),
                Err(error) => {
                    let diagnostic = Diagnostic::new(
                        stage.id(),
                        Severity::Error,
                        "",
                        0,
                        0,
                        format!("stage error: {error}"),
                    );
                    StageReport::failed(
                        stage.id(),
                        stage.kind(),
                        elapsed_ms(started),
                        vec![diagnostic],
                    )
                }
            };
            reports.push(report);
        }

        let mut report = GateReport::from_stages(ctx.target.describe(), reports);
        if self.stages.is_empty() {
            report
                .warnings
                .push("pipeline contains no stages; nothing was verified".to_string());
        }
        for stage in &report.stages {
            if stage.kind == GateKind::Blocking && stage.status == StageStatus::Skipped {
                let reason = stage
                    .skipped_reason
                    .clone()
                    .unwrap_or_else(|| "no reason given".to_string());
                report.warnings.push(format!(
                    "blocking stage '{}' was skipped: {}",
                    stage.id, reason
                ));
            }
        }
        report
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::empty()
    }
}

fn setup_failure(ctx: &GateContext, error: GateError) -> GateReport {
    let diagnostic = Diagnostic::new(
        "setup",
        Severity::Error,
        "",
        0,
        0,
        format!("could not prepare gate target: {error}"),
    );
    GateReport::from_stages(
        ctx.target.describe(),
        vec![StageReport::failed(
            "setup",
            GateKind::Blocking,
            0,
            vec![diagnostic],
        )],
    )
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn to_report(stage: &dyn Stage, duration_ms: u64, outcome: StageOutcome) -> StageReport {
    StageReport {
        id: stage.id().to_string(),
        kind: stage.kind(),
        status: outcome.status,
        duration_ms,
        diagnostics: outcome.diagnostics,
        skipped_reason: outcome.skipped_reason,
    }
}
