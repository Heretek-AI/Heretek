mod doctor;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use heretek_gate::{GateContext, Pipeline, Target};

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
    /// Check for required and optional tooling
    Doctor,
}

#[derive(Args)]
struct GateArgs {
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
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Gate(args) => run_gate(args),
        Command::Doctor => doctor::run(),
    }
}

fn run_gate(args: GateArgs) -> ExitCode {
    let target = match (args.staged, args.worktree) {
        (_, Some(path)) => Target::Worktree(path),
        _ => Target::Staged,
    };

    let repo_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("heretek: cannot resolve working directory: {error}");
            return ExitCode::from(2);
        }
    };

    let mut ctx = GateContext::new(repo_root, target);
    ctx.baseline = args.baseline;

    let pipeline = Pipeline::empty();
    let report = pipeline.run(&ctx);

    match args.format {
        OutputFormat::Json => match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("heretek: cannot serialize report: {error}");
                return ExitCode::from(3);
            }
        },
        OutputFormat::Text => print_text_report(&report, &pipeline),
    }

    if report.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_text_report(report: &heretek_core::GateReport, pipeline: &Pipeline) {
    let target = &report.target;
    println!("heretek gate: {target}");
    let stage_ids = pipeline.stage_ids();
    if stage_ids.is_empty() {
        println!("  no stages implemented yet (skeleton pipeline)");
    } else {
        for stage in &report.stages {
            let status = format!("{:?}", stage.status).to_lowercase();
            println!("  [{status}] {} ({} ms)", stage.id, stage.duration_ms);
            for diagnostic in &stage.diagnostics {
                println!(
                    "      {}:{}:{} {} {}",
                    diagnostic.file,
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.severity.as_str(),
                    diagnostic.message
                );
            }
        }
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    println!(
        "summary: {} blocking failure(s), {} new diagnostic(s)",
        report.blocking_failures,
        report.new_diagnostic_count()
    );
}
