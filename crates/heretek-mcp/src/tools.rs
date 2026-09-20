use std::path::Path;
use std::sync::Arc;

use heretek_core::{GateKind, GateReport, HereConfig};
use heretek_gate::{Baseline, GateContext, GateError, Target};

use crate::doctor;

pub struct GateRunParams {
    pub target: Option<String>,
    pub path: Option<String>,
    pub baseline: Option<String>,
}

pub fn run_gate(repo_root: &Path, params: &GateRunParams) -> Result<GateReport, GateError> {
    let config = HereConfig::load(repo_root).map_err(|error| GateError::Failed {
        message: error.to_string(),
    })?;

    let target = match (params.target.as_deref(), params.path.as_deref()) {
        (Some("worktree"), Some(path)) => Target::Worktree(path.into()),
        _ => Target::Staged,
    };
    let baseline = params
        .baseline
        .clone()
        .or_else(|| config.gate.baseline.clone());

    let pipeline = heretek_gate::build_pipeline(repo_root, &config.gate);
    let mut ctx = GateContext::new(repo_root, target)
        .with_config(Arc::new(config.gate))
        .with_baseline(baseline.clone());
    ctx.files = heretek_gate::files::changed_files(&ctx)?;

    let report = pipeline.run(&ctx);

    if baseline.is_none() {
        return Ok(report);
    }

    let id = format!("mcp-{}", std::process::id());
    let captured = Baseline::capture(&pipeline, &ctx, &id)?;
    Ok(captured.apply(report))
}

pub fn stage_inventory(repo_root: &Path) -> Vec<serde_json::Value> {
    let config = match HereConfig::load(repo_root) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("heretek-mcp: cannot load config: {error}");
            HereConfig::default()
        }
    };
    let pipeline = heretek_gate::build_pipeline(repo_root, &config.gate);
    pipeline
        .stage_kinds()
        .into_iter()
        .map(|(id, kind)| {
            serde_json::json!({
                "id": id,
                "kind": kind_label(kind),
            })
        })
        .collect()
}

pub fn doctor_json() -> serde_json::Value {
    doctor::doctor_json()
}

fn kind_label(kind: GateKind) -> &'static str {
    match kind {
        GateKind::Blocking => "blocking",
        GateKind::Advisory => "advisory",
    }
}
