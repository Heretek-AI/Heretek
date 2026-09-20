use heretek_core::{ConfigError, GateKind, GateReport};
use heretek_gate::GateError;
use heretek_model::ModelError;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("model error: {0}")]
    Model(#[from] ModelError),
    #[error("gate error: {0}")]
    Gate(#[from] GateError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("no lane available: {0}")]
    NoLane(String),
    #[error("session error: {0}")]
    Session(String),
}

pub fn blocking_frames(report: &GateReport) -> String {
    let mut lines = Vec::new();
    for stage in &report.stages {
        if stage.kind != GateKind::Blocking {
            continue;
        }
        for diagnostic in &stage.diagnostics {
            if !diagnostic.is_new {
                continue;
            }
            let code = diagnostic.code.clone().unwrap_or_default();
            let hint = diagnostic
                .hint
                .as_ref()
                .map(|hint| format!(" hint: {hint}"))
                .unwrap_or_default();
            if diagnostic.file.is_empty() {
                lines.push(format!(
                    "[{}] {}: {}{}",
                    stage.id,
                    diagnostic.severity.as_str(),
                    diagnostic.message,
                    hint
                ));
            } else {
                lines.push(format!(
                    "[{}] {}:{}:{} {} {}: {}{}",
                    stage.id,
                    diagnostic.file,
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.severity.as_str(),
                    code,
                    diagnostic.message,
                    hint
                ));
            }
        }
    }
    lines.join("\n")
}
