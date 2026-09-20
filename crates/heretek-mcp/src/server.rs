use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use heretek_core::GateReport;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::{self, GateRunParams};

#[derive(Clone)]
pub struct HeretekServer {
    cache: Arc<Mutex<Option<GateReport>>>,
}

impl HeretekServer {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for HeretekServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GateRunArgs {
    target: Option<String>,
    path: Option<String>,
    baseline: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReportGetArgs {
    format: Option<String>,
}

#[tool_router]
impl HeretekServer {
    #[tool(
        description = "Run the deterministic Heretek gate pipeline over staged changes or a worktree and return the GateReport as JSON.",
        annotations(title = "Run gate", read_only_hint = true, idempotent_hint = true)
    )]
    async fn gate_run(&self, Parameters(args): Parameters<GateRunArgs>) -> CallToolResult {
        let repo_root = match repo_root_for(&args) {
            Ok(root) => root,
            Err(message) => return tool_error(message),
        };
        let params = GateRunParams {
            target: args.target,
            path: args.path,
            baseline: args.baseline,
        };
        match tokio::task::spawn_blocking(move || tools::run_gate(&repo_root, &params)).await {
            Ok(Ok(report)) => {
                if let Ok(mut cache) = self.cache.lock() {
                    *cache = Some(report.clone());
                }
                json_result(&report)
            }
            Ok(Err(error)) => tool_error(format!("gate run failed: {error}")),
            Err(error) => tool_error(format!("gate run task failed: {error}")),
        }
    }

    #[tool(
        description = "List the gate stages that would run, with their blocking or advisory kind.",
        annotations(
            title = "List gate stages",
            read_only_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gate_list(&self) -> CallToolResult {
        match std::env::current_dir() {
            Ok(cwd) => json_result(&tools::stage_inventory(&cwd)),
            Err(error) => tool_error(format!("cannot resolve working directory: {error}")),
        }
    }

    #[tool(
        description = "Return the most recent gate_run report without re-running the gate. Use format \"json\" (default) or \"text\".",
        annotations(
            title = "Get last report",
            read_only_hint = true,
            idempotent_hint = true
        )
    )]
    async fn report_get(&self, Parameters(args): Parameters<ReportGetArgs>) -> CallToolResult {
        let cached = match self.cache.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let Some(report) = cached else {
            return tool_error("no gate report cached; run gate_run first");
        };
        match args.format.as_deref().unwrap_or("json") {
            "json" => json_result(&report),
            "text" => CallToolResult::success(vec![ContentBlock::text(render_text(&report))]),
            other => tool_error(format!(
                "unknown format \"{other}\"; expected \"text\" or \"json\""
            )),
        }
    }

    #[tool(
        description = "Probe for required and optional tooling and report versions.",
        annotations(title = "Check tooling", read_only_hint = true, idempotent_hint = true)
    )]
    async fn doctor(&self) -> CallToolResult {
        json_result(&tools::doctor_json())
    }
}

#[tool_handler(
    name = "heretek-mcp",
    instructions = "Deterministic, read-only gate tools for a repository. Run gate_run, then report_get."
)]
impl ServerHandler for HeretekServer {}

pub async fn serve_stdio() -> anyhow::Result<()> {
    let server = HeretekServer::new();
    let running = server.serve(rmcp::transport::io::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

fn repo_root_for(args: &GateRunArgs) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir()
        .map_err(|error| format!("cannot resolve working directory: {error}"))?;
    Ok(match (args.target.as_deref(), args.path.as_deref()) {
        (Some("worktree"), Some(path)) => absolute(&cwd, path),
        _ => cwd,
    })
}

fn absolute(cwd: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    }
}

fn json_result<T: serde::Serialize>(value: &T) -> CallToolResult {
    match serde_json::to_string_pretty(value) {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(error) => tool_error(format!("cannot serialize result: {error}")),
    }
}

fn tool_error(message: impl Into<String>) -> CallToolResult {
    let payload = serde_json::json!({ "error": message.into() });
    CallToolResult::error(vec![ContentBlock::text(payload.to_string())])
}

fn render_text(report: &GateReport) -> String {
    let mut out = format!("heretek gate: {}\n", report.target);
    for stage in &report.stages {
        let status = format!("{:?}", stage.status).to_lowercase();
        out.push_str(&format!(
            "  [{status}] {} ({} ms)\n",
            stage.id, stage.duration_ms
        ));
        for diagnostic in &stage.diagnostics {
            out.push_str(&format!(
                "      {}:{}:{} {} {}\n",
                diagnostic.file,
                diagnostic.line,
                diagnostic.column,
                diagnostic.severity.as_str(),
                diagnostic.message
            ));
        }
    }
    for warning in &report.warnings {
        out.push_str(&format!("warning: {warning}\n"));
    }
    out.push_str(&format!(
        "summary: {} blocking failure(s), {} new diagnostic(s)",
        report.blocking_failures,
        report.new_diagnostic_count()
    ));
    out
}
