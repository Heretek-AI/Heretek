use std::path::{Path, PathBuf};
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

    let target = resolve_target(params)?;
    let baseline = params
        .baseline
        .clone()
        .or_else(|| config.gate.baseline.clone());

    let pipeline = heretek_gate::build_pipeline(repo_root, &config.gate);
    let ctx = GateContext::new(repo_root, target)
        .with_config(Arc::new(config.gate))
        .with_baseline(baseline.clone())
        .with_files(Vec::new());
    let files = heretek_gate::files::changed_files(&ctx)?;
    let ctx = ctx.with_files(files);

    let report = pipeline.run(&ctx);

    if baseline.is_none() {
        return Ok(report);
    }

    let id = format!("mcp-{}", std::process::id());
    let captured = Baseline::capture(&pipeline, &ctx, &id)?;
    Ok(captured.match_new(report))
}

fn resolve_target(params: &GateRunParams) -> Result<Target, GateError> {
    match (params.target.as_deref(), params.path.as_deref()) {
        (None, _) | (Some("staged"), _) => Ok(Target::Staged),
        (Some("worktree"), Some(path)) => {
            let path = PathBuf::from(path);
            let absolute = if path.is_absolute() {
                path
            } else {
                std::env::current_dir().map_err(GateError::Io)?.join(path)
            };
            Ok(Target::Worktree(absolute))
        }
        (Some("worktree"), None) => Err(GateError::Failed {
            message: "target 'worktree' requires a path".to_string(),
        }),
        (Some(other), _) => Err(GateError::Failed {
            message: format!("unknown target '{other}'; expected 'staged' or 'worktree'"),
        }),
    }
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
    let mut value = doctor::doctor_json();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "network_isolation".to_string(),
            serde_json::json!(heretek_gate::process::network_isolation_available()),
        );
    }
    value
}

fn kind_label(kind: GateKind) -> &'static str {
    match kind {
        GateKind::Blocking => "blocking",
        GateKind::Advisory => "advisory",
    }
}
