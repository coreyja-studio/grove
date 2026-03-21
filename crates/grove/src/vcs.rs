use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Information about a worktree/workspace managed by a VCS backend.
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
}

/// Override for VCS backend selection via `--vcs` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcsOverride {
    Git,
}

/// Trait for VCS backends that manage worktrees/workspaces.
pub trait VcsBackend {
    fn create_worktree(&self, repo_path: &Path, worktree_path: &Path, name: &str) -> Result<()>;
    fn remove_worktree(&self, repo_path: &Path, worktree_path: &Path, name: &str) -> Result<()>;
    fn list_worktrees(&self, repo_path: &Path, worktree_base: &Path) -> Result<Vec<WorktreeInfo>>;
}

/// Detect the appropriate VCS backend for a repository.
///
/// If `vcs_override` is `Some(VcsOverride::Git)`, always uses git.
/// Otherwise, checks for `.jj` directory first (colocated repos),
/// then falls back to git.
pub fn detect_backend(
    repo_path: &Path,
    vcs_override: Option<VcsOverride>,
) -> Result<Box<dyn VcsBackend>> {
    if vcs_override == Some(VcsOverride::Git) {
        return Ok(Box::new(GitBackend));
    }
    if repo_path.join(".jj").is_dir() {
        ensure_jj_installed()?;
        return Ok(Box::new(JjBackend));
    }
    Ok(Box::new(GitBackend))
}

fn ensure_jj_installed() -> Result<()> {
    match std::process::Command::new("jj").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(Error::JjNotInstalled),
    }
}

struct GitBackend;

impl VcsBackend for GitBackend {
    fn create_worktree(&self, repo_path: &Path, worktree_path: &Path, name: &str) -> Result<()> {
        let output = std::process::Command::new("git")
            .args([
                "worktree",
                "add",
                worktree_path.to_str().unwrap(),
                "-b",
                name,
            ])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("already exists") {
                let output2 = std::process::Command::new("git")
                    .args(["worktree", "add", worktree_path.to_str().unwrap(), name])
                    .current_dir(repo_path)
                    .output()?;

                if !output2.status.success() {
                    return Err(Error::VcsCommandFailed(
                        String::from_utf8_lossy(&output2.stderr).to_string(),
                    ));
                }
            } else {
                return Err(Error::VcsCommandFailed(stderr.to_string()));
            }
        }

        Ok(())
    }

    fn remove_worktree(&self, repo_path: &Path, worktree_path: &Path, _name: &str) -> Result<()> {
        let output = std::process::Command::new("git")
            .args(["worktree", "remove", worktree_path.to_str().unwrap()])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            return Err(Error::VcsCommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(())
    }

    fn list_worktrees(&self, repo_path: &Path, _worktree_base: &Path) -> Result<Vec<WorktreeInfo>> {
        // Check if .git/worktrees exists - if not, no worktrees
        if !repo_path.join(".git/worktrees").exists() {
            return Ok(Vec::new());
        }

        let output = std::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            return Err(Error::VcsCommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch: Option<String> = None;
        let mut is_first = true;

        for line in stdout.lines() {
            if let Some(path_str) = line.strip_prefix("worktree ") {
                // Save previous worktree if any (skip first which is main repo)
                if let Some(path) = current_path.take() {
                    if !is_first {
                        worktrees.push(WorktreeInfo {
                            path,
                            branch: current_branch.take(),
                        });
                    }
                    is_first = false;
                }
                current_path = Some(PathBuf::from(path_str));
                current_branch = None;
            } else if let Some(branch_ref) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch_ref.to_string());
            }
        }
        // Don't forget the last worktree
        if let Some(path) = current_path {
            if !is_first {
                worktrees.push(WorktreeInfo {
                    path,
                    branch: current_branch,
                });
            }
        }

        Ok(worktrees)
    }
}

struct JjBackend;

impl VcsBackend for JjBackend {
    fn create_worktree(&self, repo_path: &Path, worktree_path: &Path, name: &str) -> Result<()> {
        let output = std::process::Command::new("jj")
            .args([
                "workspace",
                "add",
                "--name",
                name,
                worktree_path.to_str().unwrap(),
            ])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            return Err(Error::VcsCommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(())
    }

    fn remove_worktree(&self, repo_path: &Path, worktree_path: &Path, name: &str) -> Result<()> {
        let output = std::process::Command::new("jj")
            .args(["workspace", "forget", name])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            return Err(Error::VcsCommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // jj workspace forget does not delete the directory
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path)?;
        }

        Ok(())
    }

    fn list_worktrees(&self, repo_path: &Path, worktree_base: &Path) -> Result<Vec<WorktreeInfo>> {
        let output = std::process::Command::new("jj")
            .args(["workspace", "list"])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            return Err(Error::VcsCommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();

        for line in stdout.lines() {
            // Format: <name>: <change_id> <description>
            let Some((name, _rest)) = line.split_once(": ") else {
                continue;
            };

            // Skip the default workspace (main working directory)
            if name == "default" {
                continue;
            }

            worktrees.push(WorktreeInfo {
                path: worktree_base.join(name),
                branch: None,
            });
        }

        Ok(worktrees)
    }
}
