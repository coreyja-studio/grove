use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub projects: BTreeMap<String, Project>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub path: PathBuf,
    #[serde(default)]
    pub worktree_base: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

impl Project {
    /// Get the worktree base directory for this project.
    /// Returns the configured `worktree_base`, or defaults to `<project_path>/.worktrees`.
    pub fn worktree_base(&self) -> PathBuf {
        self.worktree_base
            .clone()
            .unwrap_or_else(|| self.path.join(".worktrees"))
    }

    /// List all worktrees for this project by running `git worktree list --porcelain`.
    /// Returns only secondary worktrees, not the main working directory.
    pub fn list_worktrees(&self) -> Result<Vec<Worktree>> {
        // Check if .git/worktrees exists - if not, no worktrees
        if !self.path.join(".git/worktrees").exists() {
            return Ok(Vec::new());
        }

        let output = std::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.path)
            .output()?;

        if !output.status.success() {
            return Err(Error::GitCommandFailed(
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
                        worktrees.push(Worktree {
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
                worktrees.push(Worktree {
                    path,
                    branch: current_branch,
                });
            }
        }

        Ok(worktrees)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EnvVars {
    #[serde(flatten)]
    pub vars: BTreeMap<String, String>,
}

fn config_dir() -> Result<PathBuf> {
    // Allow override via GROVE_CONFIG_DIR for testing
    if let Ok(dir) = std::env::var("GROVE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    dirs::config_dir()
        .map(|p| p.join("grove"))
        .ok_or(Error::NoConfigDir)
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

fn envs_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("envs"))
}

fn env_path(project: &str) -> Result<PathBuf> {
    Ok(envs_dir()?.join(format!("{project}.toml")))
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn add_project(&mut self, name: String, path: PathBuf) -> Result<()> {
        if self.projects.contains_key(&name) {
            return Err(Error::ProjectExists(name));
        }

        // Validate path exists and is a git repo
        if !path.exists() {
            return Err(Error::PathNotFound(path));
        }
        if !path.join(".git").exists() {
            return Err(Error::NotGitRepo(path));
        }

        let canonical = path.canonicalize()?;
        self.projects.insert(
            name,
            Project {
                path: canonical,
                worktree_base: None,
            },
        );
        Ok(())
    }

    pub fn remove_project(&mut self, name: &str) -> Result<()> {
        if self.projects.remove(name).is_none() {
            return Err(Error::ProjectNotFound(name.to_string()));
        }
        Ok(())
    }

    /// Find which project a path belongs to.
    ///
    /// Checks in order:
    /// 1. Path is a subdirectory of a project's main repo
    /// 2. Path is a subdirectory of one of the project's worktrees
    pub fn find_project_for_path(&self, path: &Path) -> Result<Option<String>> {
        let canonical = path.canonicalize()?;

        // First: check if path is in a project's main repo
        for (name, project) in &self.projects {
            if canonical.starts_with(&project.path) {
                return Ok(Some(name.clone()));
            }
        }

        // Second: check if path is in any project's worktree
        for (name, project) in &self.projects {
            let worktrees = project.list_worktrees()?;
            for wt in worktrees {
                if canonical.starts_with(&wt.path) {
                    return Ok(Some(name.clone()));
                }
            }
        }

        Ok(None)
    }
}

impl EnvVars {
    pub fn load(project: &str) -> Result<Self> {
        let path = env_path(project)?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        let vars: EnvVars = toml::from_str(&content)?;
        Ok(vars)
    }

    pub fn save(&self, project: &str) -> Result<()> {
        let path = env_path(project)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn set(&mut self, key: String, value: String) {
        self.vars.insert(key, value);
    }

    pub fn export(&self) -> String {
        self.vars
            .iter()
            .map(|(k, v)| format!("export {k}={}", shell_escape(v)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn shell_escape(s: &str) -> String {
    // Always quote for consistency and safety
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape_simple() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("/path/to/thing"), "'/path/to/thing'");
    }

    #[test]
    fn test_shell_escape_special() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("it's"), "'it'\"'\"'s'");
    }
}
