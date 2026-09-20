use std::process::ExitCode;

use crate::InitArgs;

const LEFTHOOK_SNIPPET: &str = r#"pre-commit:
  commands:
    heretek:
      run: heretek gate --staged --format text

pre-push:
  commands:
    heretek:
      run: heretek gate --baseline origin/main --format text
"#;

const CONFIG_TEMPLATE: &str = r#"# Heretek configuration. See docs/spec/0001-heretek-architecture.md.

[gate]
# Baseline ref for diff-aware blocking. Only new diagnostics block.
# baseline = "origin/main"
stage_timeout_secs = 300
# allow_network = false
# stages.tests = false

[agent]
max_turns = 40
max_wall_secs = 1800

# Any OpenAI-compatible endpoint works.
[models.fast]
lane = "fast"
base_url = "http://127.0.0.1:8080/v1"
model = "qwen3.6-35b-a3b"
context_tokens = 32768

# [models.deep]
# lane = "deep"
# base_url = "http://127.0.0.1:8080/v1"
# model = "qwen3.6-122b-a10b"
# context_tokens = 131072
"#;

pub fn run(args: &InitArgs) -> ExitCode {
    if args.print {
        print!("{LEFTHOOK_SNIPPET}");
        return ExitCode::SUCCESS;
    }

    let repo_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("heretek: cannot resolve working directory: {error}");
            return ExitCode::from(3);
        }
    };

    let config_path = repo_root.join(".heretek.toml");
    if config_path.exists() && !args.force {
        println!(
            "heretek: {} already exists (use --force)",
            config_path.display()
        );
    } else if let Err(error) = std::fs::write(&config_path, CONFIG_TEMPLATE) {
        eprintln!("heretek: cannot write {}: {error}", config_path.display());
        return ExitCode::from(3);
    } else {
        println!("heretek: wrote {}", config_path.display());
    }

    if args.lefthook {
        let hook_path = repo_root.join("lefthook.yml");
        if hook_path.exists() && !args.force {
            println!(
                "heretek: {} already exists; add the snippet from `heretek init --print`",
                hook_path.display()
            );
        } else if let Err(error) = std::fs::write(&hook_path, LEFTHOOK_SNIPPET) {
            eprintln!("heretek: cannot write {}: {error}", hook_path.display());
            return ExitCode::from(3);
        } else {
            println!("heretek: wrote {}", hook_path.display());
        }
    }

    println!("next: `heretek doctor` to check tools, then `heretek gate --staged`");
    ExitCode::SUCCESS
}
