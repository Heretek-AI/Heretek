use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::files::git_output;
use crate::stage::GateError;

pub struct ShadowWorkspace {
    path: PathBuf,
    id: String,
    git: bool,
    base: String,
    snapshots: Vec<String>,
}

impl ShadowWorkspace {
    pub fn create(repo_root: &Path, id: &str) -> Result<Self, GateError> {
        let git = is_git_repo(repo_root);
        let heretek_dir = repo_root.join(".heretek");
        std::fs::create_dir_all(&heretek_dir)?;

        if git {
            let path = heretek_dir.join("worktrees").join(id);
            if path.exists() {
                let _ = git_output(
                    repo_root,
                    &[
                        "worktree".to_string(),
                        "remove".to_string(),
                        "--force".to_string(),
                        path.display().to_string(),
                    ],
                );
                let _ = std::fs::remove_dir_all(&path);
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            git_output(
                repo_root,
                &[
                    "worktree".to_string(),
                    "add".to_string(),
                    "--detach".to_string(),
                    path.display().to_string(),
                    "HEAD".to_string(),
                ],
            )?;
            let base = git_output(&path, &["rev-parse".to_string(), "HEAD".to_string()])?
                .trim()
                .to_string();
            Ok(Self {
                path,
                id: id.to_string(),
                git: true,
                base,
                snapshots: Vec::new(),
            })
        } else {
            let path = heretek_dir.join("shadow").join(id);
            if path.exists() {
                std::fs::remove_dir_all(&path)?;
            }
            copy_tree(repo_root, &path)?;
            Ok(Self {
                path,
                id: id.to_string(),
                git: false,
                base: String::new(),
                snapshots: Vec::new(),
            })
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_git(&self) -> bool {
        self.git
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn snapshot(&mut self, label: &str) -> Result<String, GateError> {
        if !self.git {
            let marker = format!("copy-{}", self.snapshots.len());
            self.snapshots.push(marker.clone());
            return Ok(marker);
        }
        git_output(&self.path, &["add".to_string(), "-A".to_string()])?;
        git_output(
            &self.path,
            &[
                "-c".to_string(),
                "user.name=heretek".to_string(),
                "-c".to_string(),
                "user.email=heretek@localhost".to_string(),
                "commit".to_string(),
                "--no-verify".to_string(),
                "--allow-empty".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                format!("heretek snapshot: {label}"),
            ],
        )?;
        let sha = git_output(&self.path, &["rev-parse".to_string(), "HEAD".to_string()])?
            .trim()
            .to_string();
        let refname = format!("refs/heretek/turns/{}/{}", self.id, self.snapshots.len());
        let _ = git_output(
            &self.path,
            &["update-ref".to_string(), refname, sha.clone()],
        );
        self.snapshots.push(sha.clone());
        Ok(sha)
    }

    pub fn rollback(&self, sha: &str) -> Result<(), GateError> {
        if !self.git {
            return Err(GateError::Failed {
                message: "rollback is not supported for non-git workspaces".to_string(),
            });
        }
        git_output(
            &self.path,
            &["reset".to_string(), "--hard".to_string(), sha.to_string()],
        )?;
        git_output(&self.path, &["clean".to_string(), "-fdq".to_string()])?;
        Ok(())
    }

    pub fn diff(&self) -> Result<String, GateError> {
        if !self.git {
            return Err(GateError::Failed {
                message: "diff is not supported for non-git workspaces".to_string(),
            });
        }
        git_output(
            &self.path,
            &[
                "diff".to_string(),
                "--binary".to_string(),
                self.base.clone(),
            ],
        )
    }

    pub fn apply_to(&self, target: &Path) -> Result<(), GateError> {
        if self.git {
            let patch = self.diff()?;
            if patch.trim().is_empty() {
                return Ok(());
            }
            return git_apply(target, &patch);
        }
        copy_tree(&self.path, target)
    }

    pub fn cleanup(self) {
        if self.git {
            let _ = git_output(
                std::path::Path::new("."),
                &[
                    "worktree".to_string(),
                    "remove".to_string(),
                    "--force".to_string(),
                    self.path.display().to_string(),
                ],
            );
        } else {
            let _ = std::fs::remove_dir_all(&self.path);
        }
        let _ = self.snapshots;
    }
}

pub fn is_git_repo(path: &Path) -> bool {
    git_output(
        path,
        &["rev-parse".to_string(), "--is-inside-work-tree".to_string()],
    )
    .map(|output| output.trim() == "true")
    .unwrap_or(false)
}

fn git_apply(target: &Path, patch: &str) -> Result<(), GateError> {
    use std::io::Write;
    let mut child = std::process::Command::new("git")
        .args(["apply", "--whitespace=nowarn", "--recount", "-"])
        .current_dir(target)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GateError::Io)?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(patch.as_bytes()).map_err(GateError::Io)?;
    }
    let output = child.wait_with_output().map_err(GateError::Io)?;
    if !output.status.success() {
        return Err(GateError::Failed {
            message: format!(
                "git apply failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), GateError> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name == ".git" || name == ".heretek" || name == "node_modules" {
            continue;
        }
        let from = entry.path();
        let to = destination.join(&file_name);
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else if from.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
