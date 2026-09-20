use heretek_core::{Diagnostic, GateKind, Severity};
use heretek_gate::{GateContext, GateError, Pipeline, Stage, StageOutcome, Target};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp/heretek-test")
}

fn context() -> GateContext {
    GateContext::new(
        repo_root(),
        Target::Worktree(std::path::PathBuf::from("/tmp/heretek-test/tree")),
    )
}

#[derive(Clone, Copy)]
struct FakeStage {
    id: &'static str,
    kind: GateKind,
    outcome: FakeOutcome,
}

#[derive(Clone, Copy)]
enum FakeOutcome {
    Pass,
    Fail,
    Skip,
    Error,
}

impl Stage for FakeStage {
    fn id(&self) -> &'static str {
        self.id
    }

    fn kind(&self) -> GateKind {
        self.kind
    }

    fn run(&self, _ctx: &GateContext) -> Result<StageOutcome, GateError> {
        match self.outcome {
            FakeOutcome::Pass => Ok(StageOutcome::passed()),
            FakeOutcome::Fail => Ok(StageOutcome::failed(vec![Diagnostic::new(
                self.id,
                Severity::Error,
                "src/lib.ts",
                1,
                1,
                "fake failure",
            )])),
            FakeOutcome::Skip => Ok(StageOutcome::skipped("not applicable")),
            FakeOutcome::Error => Err(GateError::ToolMissing {
                tool: "fake-tool".to_string(),
            }),
        }
    }
}

fn pipeline(stages: Vec<FakeStage>) -> Pipeline {
    Pipeline::new(
        stages
            .into_iter()
            .map(|s| Box::new(s) as Box<dyn Stage>)
            .collect(),
    )
}

#[test]
fn blocking_failure_fails_the_report() {
    let report = pipeline(vec![
        FakeStage {
            id: "syntax",
            kind: GateKind::Blocking,
            outcome: FakeOutcome::Fail,
        },
        FakeStage {
            id: "format",
            kind: GateKind::Blocking,
            outcome: FakeOutcome::Pass,
        },
    ])
    .run(&context());

    assert!(!report.passed);
    assert_eq!(report.blocking_failures, 1);
    assert_eq!(report.stages.len(), 2);
    assert_eq!(report.new_diagnostic_count(), 1);
}

#[test]
fn advisory_failure_does_not_fail_the_report() {
    let report = pipeline(vec![FakeStage {
        id: "sast",
        kind: GateKind::Advisory,
        outcome: FakeOutcome::Fail,
    }])
    .run(&context());

    assert!(report.passed);
    assert_eq!(report.blocking_failures, 0);
    assert_eq!(report.new_diagnostic_count(), 1);
}

#[test]
fn skipped_stage_is_recorded_with_reason() {
    let report = pipeline(vec![FakeStage {
        id: "tests",
        kind: GateKind::Blocking,
        outcome: FakeOutcome::Skip,
    }])
    .run(&context());

    assert!(report.passed);
    assert_eq!(report.stages[0].status, heretek_core::StageStatus::Skipped);
    assert_eq!(
        report.stages[0].skipped_reason.as_deref(),
        Some("not applicable")
    );
}

#[test]
fn stage_error_becomes_a_blocking_diagnostic() {
    let report = pipeline(vec![FakeStage {
        id: "typecheck",
        kind: GateKind::Blocking,
        outcome: FakeOutcome::Error,
    }])
    .run(&context());

    assert!(!report.passed);
    assert_eq!(report.blocking_failures, 1);
    let message = &report.stages[0].diagnostics[0].message;
    assert!(message.contains("fake-tool"));
}

#[test]
fn stages_run_in_declaration_order() {
    let report = pipeline(vec![
        FakeStage {
            id: "syntax",
            kind: GateKind::Blocking,
            outcome: FakeOutcome::Pass,
        },
        FakeStage {
            id: "format",
            kind: GateKind::Blocking,
            outcome: FakeOutcome::Pass,
        },
        FakeStage {
            id: "tests",
            kind: GateKind::Blocking,
            outcome: FakeOutcome::Pass,
        },
    ])
    .run(&context());

    let ids: Vec<&str> = report.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["syntax", "format", "tests"]);
}

#[test]
fn empty_pipeline_warns_instead_of_silently_passing() {
    let report = Pipeline::empty().run(&context());

    assert!(report.passed);
    assert_eq!(
        report.warnings,
        ["pipeline contains no stages; nothing was verified"]
    );
}
