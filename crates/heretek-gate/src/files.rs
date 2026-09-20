use std::path::{Path, PathBuf};
use std::process::Command;

use heretek_core::validate_git_ref;

use crate::stage::{GateContext, GateError, Target};

const DEFAULT_IGNORES: &[&str] = &[
    "node_modules/",
    "target/",
    "dist/",
    "build/",
    "vendor/",
    ".heretek/",
    ".heretek-audit/",
    ".heretek-shadow.json",
    ".git/",
    "coverage/",
];

pub fn changed_files(ctx: &GateContext) -> Result<Vec<String>, GateError> {
    ctx.validate()?;
    let mut files: Vec<String> = Vec::new();

    match &ctx.target {
        Target::Staged => match &ctx.baseline {
            Some(baseline) => {
                files.extend(nul_to_paths(&git_output(
                    &ctx.repo_root,
                    &[
                        "diff".to_string(),
                        "--cached".to_string(),
                        "--name-only".to_string(),
                        "--diff-filter=ACMR".to_string(),
                        "-z".to_string(),
                        baseline.clone(),
                        "--".to_string(),
                    ],
                )?));
            }
            None => {
                files.extend(nul_to_paths(&git_output(
                    &ctx.repo_root,
                    &[
                        "diff".to_string(),
                        "--cached".to_string(),
                        "--name-only".to_string(),
                        "--diff-filter=ACMR".to_string(),
                        "-z".to_string(),
                    ],
                )?));
            }
        },
        Target::Worktree(path) => {
            let mut args = vec![
                "diff".to_string(),
                "--name-only".to_string(),
                "-z".to_string(),
            ];
            if let Some(baseline) = &ctx.baseline {
                args.push(baseline.clone());
                args.push("--".to_string());
            }
            if ctx.baseline.is_some() {
                files.extend(nul_to_paths(&git_output(path, &args)?));
            }
            files.extend(nul_to_paths(&git_output(
                path,
                &[
                    "ls-files".to_string(),
                    "-z".to_string(),
                    "--cached".to_string(),
                    "--others".to_string(),
                    "--exclude-standard".to_string(),
                ],
            )?));
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    let mut output = Vec::new();
    for file in files {
        if file.is_empty() || is_ignored(&file, &ctx.config.ignore) {
            continue;
        }
        if seen.insert(file.clone()) {
            output.push(file);
        }
    }
    Ok(output)
}

fn nul_to_paths(output: &str) -> Vec<String> {
    output
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn git_output(cwd: &Path, args: &[String]) -> Result<String, GateError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(GateError::Io)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GateError::Failed {
            message: format!("git {} failed: {}", args.join(" "), stderr.trim()),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn is_ignored(path: &str, extra: &[String]) -> bool {
    DEFAULT_IGNORES
        .iter()
        .map(|pattern| (*pattern).to_string())
        .chain(extra.iter().cloned())
        .any(|pattern| matches_pattern(path, &pattern))
}

fn matches_pattern(path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    if let Some(prefix) = pattern.strip_suffix('/') {
        return path.starts_with(prefix) || path.split('/').any(|segment| segment == prefix);
    }
    if pattern.contains('*') {
        return wildcard_match(path, pattern);
    }
    path == pattern
        || path.starts_with(&format!("{pattern}/"))
        || path.split('/').any(|segment| segment == pattern)
}

fn wildcard_match(text: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }
    let mut remainder = text;
    if let Some(first) = parts.first()
        && !first.is_empty()
    {
        let Some(stripped) = remainder.strip_prefix(first) else {
            return false;
        };
        remainder = stripped;
    }
    for (index, part) in parts.iter().enumerate().skip(1) {
        if part.is_empty() {
            continue;
        }
        if index == parts.len() - 1 {
            return remainder.ends_with(part);
        }
        let Some(position) = remainder.find(part) else {
            return false;
        };
        remainder = &remainder[position + part.len()..];
    }
    true
}

pub fn is_source_file(path: &str, extensions: &[&str]) -> bool {
    extensions.iter().any(|extension| path.ends_with(extension))
}

pub fn resolve_tool(repo_root: &Path, name: &str) -> Option<PathBuf> {
    let local = repo_root.join("node_modules").join(".bin").join(name);
    if local.is_file() {
        return Some(local);
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

pub fn has_tool(repo_root: &Path, name: &str) -> bool {
    resolve_tool(repo_root, name).is_some()
}

pub fn display_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub fn valid_ref(value: &str) -> Result<(), GateError> {
    validate_git_ref(value).map_err(|message| GateError::Failed { message })
}
