use heretek_core::{GateKind, GateReport, StageStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Verified,
    Unverified,
    NotApplicable,
}

pub fn verification(report: &GateReport, changed_files: &[String]) -> Verification {
    let blocking_ran = report
        .stages
        .iter()
        .any(|stage| stage.kind == GateKind::Blocking && stage.status != StageStatus::Skipped);
    if blocking_ran {
        return Verification::Verified;
    }
    let gateable = changed_files
        .iter()
        .any(|file| crate::stages::util::is_ts(file));
    if report.passed && (!gateable || changed_files.is_empty()) {
        return Verification::NotApplicable;
    }
    Verification::Unverified
}

pub fn is_verified(report: &GateReport, changed_files: &[String]) -> bool {
    verification(report, changed_files) != Verification::Unverified
}
