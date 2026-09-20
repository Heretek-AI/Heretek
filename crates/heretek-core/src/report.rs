use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, GateKind};

pub const REPORT_SCHEMA: &str = "heretek.report/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StageStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageReport {
    pub id: String,
    pub kind: GateKind,
    pub status: StageStatus,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
}

impl StageReport {
    pub fn passed(id: impl Into<String>, kind: GateKind, duration_ms: u64) -> Self {
        Self {
            id: id.into(),
            kind,
            status: StageStatus::Passed,
            duration_ms,
            diagnostics: Vec::new(),
            skipped_reason: None,
        }
    }

    pub fn failed(
        id: impl Into<String>,
        kind: GateKind,
        duration_ms: u64,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            status: StageStatus::Failed,
            duration_ms,
            diagnostics,
            skipped_reason: None,
        }
    }

    pub fn skipped(id: impl Into<String>, kind: GateKind, reason: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            status: StageStatus::Skipped,
            duration_ms: 0,
            diagnostics: Vec::new(),
            skipped_reason: Some(reason.into()),
        }
    }

    pub fn new_diagnostic_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_new).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateReport {
    pub schema: String,
    pub target: String,
    pub stages: Vec<StageReport>,
    pub passed: bool,
    pub blocking_failures: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl GateReport {
    pub fn from_stages(target: impl Into<String>, stages: Vec<StageReport>) -> Self {
        let blocking_failures = stages
            .iter()
            .filter(|stage| {
                stage.kind == GateKind::Blocking
                    && stage.status == StageStatus::Failed
                    && stage.new_diagnostic_count() > 0
            })
            .count();
        Self {
            schema: REPORT_SCHEMA.to_string(),
            target: target.into(),
            stages,
            passed: blocking_failures == 0,
            blocking_failures,
            warnings: Vec::new(),
        }
    }

    pub fn new_diagnostic_count(&self) -> usize {
        self.stages
            .iter()
            .map(StageReport::new_diagnostic_count)
            .sum()
    }
}
