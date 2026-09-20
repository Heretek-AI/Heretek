use std::path::{Path, PathBuf};
use std::process::Command;

use crate::stage::{GateContext, GateError, Target};

const DEFAULT_IGNORES: &[&str] = &[
    "node_modules/",
    "target/",
    "dist/",
    "build/",
    "vendor/",
    ".heretek/",
    ".git/",
    "coverage/",
];

pub fn changed_files(ctx: &GateContext) -> Result<Vec<String>, GateError> {
    let (cwd, args) = match &ctx.target {
        Target::Staged => (
            ctx.repo_root.clone(),
            vec![
                "diff".to_string(),
                "--cached".to_string(),
                "--name-only".to_string(),
                "--diff-filter=ACMR".to_string(),
                "-z".to_string(),
            ],
        ),
        Target::Worktree(path) => match &ctx.baseline {
            Some(baseline) => (
                path.clone(),
                vec![
                    "diff".to_string(),
                    "--name-only".to_string(),
                    "-z".to_string(),
                    baseline.clone(),
                ],
            ),
            None => (path.clone(), vec!["ls-files".to_string(), "-z".to_string()]),
        },
    };

    let output = git_output(&cwd, &args)?;
    let mut files = Vec::new();
    for entry in output.split('\0') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if is_ignored(entry, &ctx.config.ignore) {
            continue;
        }
        files.push(entry.to_string());
    }
    Ok(files)
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
    path == pattern || path.ends_with(pattern) || path.contains(pattern)
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
