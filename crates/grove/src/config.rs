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
        self.projects.insert(name, Project { path: canonical });
        Ok(())
    }

    pub fn remove_project(&mut self, name: &str) -> Result<()> {
        if self.projects.remove(name).is_none() {
            return Err(Error::ProjectNotFound(name.to_string()));
        }
        Ok(())
    }

    /// Find which project a path belongs to (exact match or subdirectory)
    pub fn find_project_for_path(&self, path: &Path) -> Result<Option<String>> {
        let canonical = path.canonicalize()?;

        for (name, project) in &self.projects {
            if canonical.starts_with(&project.path) {
                return Ok(Some(name.clone()));
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
