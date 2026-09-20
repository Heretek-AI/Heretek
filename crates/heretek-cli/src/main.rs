use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use heretek_core::{GateReport, HereConfig};
use heretek_gate::{Baseline, GateContext, Pipeline, Target, build_pipeline};

const EXIT_PASS: u8 = 0;
const EXIT_BLOCKED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_INTERNAL: u8 = 3;

#[derive(Parser)]
#[command(
    name = "heretek",
    version,
    about = "Local-first coding harness with deterministic verification gates"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the deterministic gate pipeline
    Gate(GateArgs),
    /// Run an agent session against the workspace
    Run(RunArgs),
    /// Check tooling, network isolation, and configured model endpoints
    Doctor(DoctorArgs),
    /// Write starter configuration and hook files
    Init(InitArgs),
    /// Apply a persisted shadow result to the working tree
    Apply(ApplyArgs),
    /// Remove all persisted shadows and stale worktree registrations
    Clean,
    /// Run the MCP server on stdio
    Mcp,
}

#[derive(Args)]
pub struct ApplyArgs {
    /// Path to the shadow directory (printed by `heretek run`)
    pub path: PathBuf,
}

#[derive(Args)]
pub struct RunArgs {
    /// Task description for the agent
    pub task: String,
    /// Model lane from .heretek.toml
    #[arg(long, value_name = "LANE")]
    pub lane: Option<String>,
    /// Maximum turns before the session stops
    #[arg(long, value_name = "N")]
    pub max_turns: Option<u32>,
    /// Apply the shadow result to the working tree if the final gate passes
    #[arg(long)]
    pub apply: bool,
}

#[derive(Args)]
pub struct GateArgs {
    /// Gate the staged changes (default target)
    #[arg(long)]
    staged: bool,
    /// Gate the worktree at this path
    #[arg(long, value_name = "PATH")]
    worktree: Option<PathBuf>,
    /// Baseline git ref used for diff-aware blocking
    #[arg(long, value_name = "REF")]
    baseline: Option<String>,
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    /// Allow the format stage to auto-fix (harness-owned worktrees only)
    #[arg(long)]
    fix: bool,
}

#[derive(Args)]
pub struct DoctorArgs {
    /// Probe every configured model endpoint
    #[arg(long)]
    models: bool,
}

#[derive(Args)]
pub struct InitArgs {
    /// Emit the lefthook snippet on stdout instead of writing files
    #[arg(long)]
    print: bool,
    /// Overwrite existing files
    #[arg(long)]
    force: bool,
    /// Also write the lefthook snippet
    #[arg(long)]
    lefthook: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Gate(args) => run_gate(&args),
        Command::Run(args) => run_agent(&args),
        Command::Doctor(args) => doctor::run(&args),
        Command::Init(args) => init::run(&args),
        Command::Apply(args) => run_apply(&args),
        Command::Clean => run_clean(),
        Command::Mcp => run_mcp(),
    }
}

fn run_apply(args: &ApplyArgs) -> ExitCode {
    let repo_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("heretek: cannot resolve working directory: {error}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };
    let path = if args.path.is_absolute() {
        args.path.clone()
    } else {
        repo_root.join(&args.path)
    };
    match heretek_gate::apply_from(&repo_root, &path) {
        Ok(()) => {
            println!("heretek: applied {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("heretek: {error}");
            ExitCode::from(EXIT_BLOCKED)
        }
    }
}

fn run_clean() -> ExitCode {
    let repo_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("heretek: cannot resolve working directory: {error}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };
    let mut removed = 0usize;
    for base in ["worktrees", "shadow"] {
        let dir = repo_root.join(".heretek").join(base);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if heretek_gate::discard(&repo_root, &path).is_ok() {
                removed += 1;
            }
        }
    }
    println!("heretek: removed {removed} shadow(s)");
    ExitCode::SUCCESS
}

fn run_agent(args: &RunArgs) -> ExitCode {
    let repo_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("heretek: cannot resolve working directory: {error}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };
    let options = heretek_agent::SessionOptions {
        lane: args.lane.clone(),
        max_turns: args.max_turns,
        apply: args.apply,
    };
    match heretek_agent::run_session(&repo_root, &args.task, &options) {
        Ok(outcome) => {
            println!("heretek run: {} turn(s)", outcome.turns);
            println!(
                "  tokens: {} prompt, {} completion, {} cached",
                outcome.usage.prompt_tokens,
                outcome.usage.completion_tokens,
                outcome.usage.cached_tokens.unwrap_or(0)
            );
            println!("  shadow: {}", outcome.shadow_path.display());
            println!("  events: {}", outcome.events_path.display());
            if outcome.escalated {
                println!("  escalated to the deep lane during the session");
            }
            if !outcome.summary.is_empty() {
                println!("  summary: {}", outcome.summary);
            }
            println!(
                "  final gate: {} (verification: {})",
                if outcome.passed { "passed" } else { "failed" },
                if outcome.verified {
                    "at least one blocking stage ran"
                } else {
                    "no blocking stage ran; treat as unverified"
                }
            );
            if outcome.finished && outcome.passed {
                ExitCode::from(EXIT_PASS)
            } else {
                ExitCode::from(EXIT_BLOCKED)
            }
        }
        Err(error) => {
            eprintln!("heretek: {error}");
            ExitCode::from(EXIT_INTERNAL)
        }
    }
}

fn run_gate(args: &GateArgs) -> ExitCode {
    let repo_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("heretek: cannot resolve working directory: {error}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };
    let config = match HereConfig::load(&repo_root) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("heretek: {error}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let target = match (&args.staged, &args.worktree) {
        (_, Some(path)) => {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                repo_root.join(path)
            };
            Target::Worktree(absolute)
        }
        _ => Target::Staged,
    };

    let baseline = args
        .baseline
        .clone()
        .or_else(|| config.gate.baseline.clone());
    let pipeline = build_pipeline(&repo_root, &config.gate);
    let ctx = GateContext::new(&repo_root, target)
        .with_config(std::sync::Arc::new(config.gate.clone()))
        .with_baseline(baseline.clone())
        .with_fix(args.fix);

    let files = match heretek_gate::files::changed_files(&ctx) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("heretek: {error}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };
    let ctx = ctx.with_files(files);

    let mut report = pipeline.run(&ctx);

    if baseline.is_some() {
        let id = format!("cli-{}", std::process::id());
        match Baseline::capture(&pipeline, &ctx, &id) {
            Ok(captured) => report = captured.match_new(report),
            Err(error) => {
                eprintln!("heretek: baseline capture failed: {error}");
                return ExitCode::from(EXIT_INTERNAL);
            }
        }
    }

    match args.format {
        OutputFormat::Json => match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("heretek: cannot serialize report: {error}");
                return ExitCode::from(EXIT_INTERNAL);
            }
        },
        OutputFormat::Text => print_text_report(&report, &pipeline),
    }

    if report.passed {
        ExitCode::from(EXIT_PASS)
    } else {
        ExitCode::from(EXIT_BLOCKED)
    }
}

fn print_text_report(report: &GateReport, pipeline: &Pipeline) {
    println!("heretek gate: {}", report.target);
    for stage in &report.stages {
        let status = format!("{:?}", stage.status).to_lowercase();
        println!("  [{status}] {} ({} ms)", stage.id, stage.duration_ms);
        if let Some(reason) = &stage.skipped_reason {
            println!("      skipped: {reason}");
        }
        for diagnostic in &stage.diagnostics {
            let marker = if diagnostic.is_new { "new" } else { "baseline" };
            println!(
                "      {}:{}:{} {} [{}] {}",
                diagnostic.file,
                diagnostic.line,
                diagnostic.column,
                diagnostic.severity.as_str(),
                marker,
                diagnostic.message
            );
        }
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    println!(
        "summary: {} blocking failure(s), {} new diagnostic(s), {} baseline diagnostic(s)",
        report.blocking_failures,
        report.new_diagnostic_count(),
        report
            .stages
            .iter()
            .flat_map(|stage| &stage.diagnostics)
            .filter(|diagnostic| !diagnostic.is_new)
            .count()
    );
    let _ = pipeline;
}

fn run_mcp() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("heretek: cannot start async runtime: {error}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };
    match runtime.block_on(heretek_mcp::serve_stdio()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("heretek: mcp server failed: {error}");
            ExitCode::from(EXIT_INTERNAL)
        }
    }
}

mod doctor;
mod init;
