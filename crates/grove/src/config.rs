use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug)]
pub struct ProjectRef {
    pub project: String,
    pub worktree: Option<String>,
}

impl ProjectRef {
    pub fn parse(input: &str) -> Result<Self> {
        let parts: Vec<&str> = input.split('/').collect();
        match parts.as_slice() {
            [project] if !project.is_empty() => Ok(Self {
                project: (*project).to_string(),
                worktree: None,
            }),
            [project, worktree] if !project.is_empty() && !worktree.is_empty() => Ok(Self {
                project: (*project).to_string(),
                worktree: Some((*worktree).to_string()),
            }),
            _ => Err(Error::InvalidProjectRef(input.to_string())),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub projects: BTreeMap<String, Project>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url_template: String,
    #[serde(default)]
    pub setup_command: Option<String>,
    #[serde(default)]
    pub env_var: Option<String>,
}

impl DatabaseConfig {
    #[allow(clippy::unused_self)]
    pub fn db_name(&self, project: &str, worktree: &str) -> String {
        let raw = format!("{project}_{worktree}");
        raw.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .to_lowercase()
    }

    pub fn database_url(&self, project: &str, worktree: &str) -> String {
        let db_name = self.db_name(project, worktree);
        self.url_template.replace("{{db_name}}", &db_name)
    }

    pub fn env_var_name(&self) -> &str {
        self.env_var.as_deref().unwrap_or("DATABASE_URL")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub path: PathBuf,
    #[serde(default)]
    pub worktree_base: Option<PathBuf>,
    #[serde(default)]
    pub database: Option<DatabaseConfig>,
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

pub(crate) fn env_path(project: &str) -> Result<PathBuf> {
    Ok(envs_dir()?.join(format!("{project}.toml")))
}

pub(crate) fn worktree_env_path(project: &str, worktree: &str) -> Result<PathBuf> {
    Ok(envs_dir()?.join(project).join(format!("{worktree}.toml")))
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
                database: None,
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
    /// 1. Path is a subdirectory of one of the project's worktrees (more specific match)
    /// 2. Path is a subdirectory of a project's main repo
    pub fn find_project_for_path(&self, path: &Path) -> Result<Option<ProjectRef>> {
        let canonical = path.canonicalize()?;

        // First: check if path is in any project's worktree (more specific match)
        for (name, project) in &self.projects {
            let worktrees = project.list_worktrees()?;
            for wt in worktrees {
                if canonical.starts_with(&wt.path) {
                    let worktree_dir_name =
                        wt.path.file_name().map(|n| n.to_string_lossy().to_string());
                    let Some(wt_name) = worktree_dir_name else {
                        continue;
                    };
                    return Ok(Some(ProjectRef {
                        project: name.clone(),
                        worktree: Some(wt_name),
                    }));
                }
            }
        }

        // Second: check if path is in a project's main repo
        for (name, project) in &self.projects {
            if canonical.starts_with(&project.path) {
                return Ok(Some(ProjectRef {
                    project: name.clone(),
                    worktree: None,
                }));
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

    pub fn load_worktree(project: &str, worktree: &str) -> Result<Self> {
        let path = worktree_env_path(project, worktree)?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        let vars: Self = toml::from_str(&content)?;
        Ok(vars)
    }

    pub fn save_worktree(&self, project: &str, worktree: &str) -> Result<()> {
        let path = worktree_env_path(project, worktree)?;
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

    pub fn remove(&mut self, key: &str) -> bool {
        self.vars.remove(key).is_some()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EnvSource {
    Project,
    Worktree,
}

#[derive(Debug)]
pub struct MergedEnvVar {
    pub key: String,
    pub value: String,
    pub source: EnvSource,
}

pub fn load_merged_env(project_ref: &ProjectRef) -> Result<Vec<MergedEnvVar>> {
    let base = EnvVars::load(&project_ref.project)?;

    let mut merged: BTreeMap<String, (String, EnvSource)> = base
        .vars
        .into_iter()
        .map(|(k, v)| (k, (v, EnvSource::Project)))
        .collect();

    if let Some(wt) = &project_ref.worktree {
        let overrides = EnvVars::load_worktree(&project_ref.project, wt)?;
        for (k, v) in overrides.vars {
            merged.insert(k, (v, EnvSource::Worktree));
        }
    }

    Ok(merged
        .into_iter()
        .map(|(key, (value, source))| MergedEnvVar { key, value, source })
        .collect())
}

pub fn export_merged_env(vars: &[MergedEnvVar]) -> String {
    vars.iter()
        .map(|var| format!("export {}={}", var.key, shell_escape(&var.value)))
        .collect::<Vec<_>>()
        .join("\n")
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

    #[test]
    fn test_project_ref_parse_project_only() {
        let pr = ProjectRef::parse("mull").unwrap();
        assert_eq!(pr.project, "mull");
        assert!(pr.worktree.is_none());
    }

    #[test]
    fn test_project_ref_parse_with_worktree() {
        let pr = ProjectRef::parse("mull/discord").unwrap();
        assert_eq!(pr.project, "mull");
        assert_eq!(pr.worktree.as_deref(), Some("discord"));
    }

    #[test]
    fn test_project_ref_parse_too_many_parts() {
        assert!(ProjectRef::parse("mull/discord/extra").is_err());
    }

    #[test]
    fn test_project_ref_parse_leading_slash() {
        assert!(ProjectRef::parse("/discord").is_err());
    }

    #[test]
    fn test_project_ref_parse_trailing_slash() {
        assert!(ProjectRef::parse("mull/").is_err());
    }

    #[test]
    fn test_project_ref_parse_empty() {
        assert!(ProjectRef::parse("").is_err());
    }

    #[test]
    fn test_db_name_basic() {
        let cfg = DatabaseConfig {
            url_template: String::new(),
            setup_command: None,
            env_var: None,
        };
        assert_eq!(cfg.db_name("mull", "feature-auth"), "mull_feature_auth");
        assert_eq!(
            cfg.db_name("my-project", "add-users"),
            "my_project_add_users"
        );
    }

    #[test]
    fn test_db_name_case_and_special_chars() {
        let cfg = DatabaseConfig {
            url_template: String::new(),
            setup_command: None,
            env_var: None,
        };
        assert_eq!(cfg.db_name("Mull", "Feature"), "mull_feature");
        assert_eq!(cfg.db_name("my.project", "feat"), "my_project_feat");
        assert_eq!(cfg.db_name("has spaces", "feat"), "has_spaces_feat");
    }

    #[test]
    fn test_database_url_template() {
        let cfg = DatabaseConfig {
            url_template: "postgres:///{{db_name}}".to_string(),
            setup_command: None,
            env_var: None,
        };
        assert_eq!(
            cfg.database_url("mull", "feature"),
            "postgres:///mull_feature"
        );

        let cfg2 = DatabaseConfig {
            url_template: "postgres://localhost:5432/{{db_name}}".to_string(),
            setup_command: None,
            env_var: None,
        };
        assert_eq!(
            cfg2.database_url("mull", "feature"),
            "postgres://localhost:5432/mull_feature"
        );
    }

    #[test]
    fn test_env_var_name_default() {
        let cfg = DatabaseConfig {
            url_template: String::new(),
            setup_command: None,
            env_var: None,
        };
        assert_eq!(cfg.env_var_name(), "DATABASE_URL");
    }

    #[test]
    fn test_env_var_name_custom() {
        let cfg = DatabaseConfig {
            url_template: String::new(),
            setup_command: None,
            env_var: Some("DB_URL".to_string()),
        };
        assert_eq!(cfg.env_var_name(), "DB_URL");
    }

    #[test]
    fn test_database_config_deserialization() {
        let toml_str = r#"
[projects.myproject]
path = "/tmp/myproject"

[projects.myproject.database]
url_template = "postgres:///{{db_name}}"
setup_command = "cargo sqlx database setup"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let project = config.projects.get("myproject").unwrap();
        let db = project.database.as_ref().unwrap();
        assert_eq!(db.url_template, "postgres:///{{db_name}}");
        assert_eq!(
            db.setup_command.as_deref(),
            Some("cargo sqlx database setup")
        );
        assert!(db.env_var.is_none());
    }

    #[test]
    fn test_database_config_absent_backward_compat() {
        let toml_str = r#"
[projects.myproject]
path = "/tmp/myproject"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let project = config.projects.get("myproject").unwrap();
        assert!(project.database.is_none());
    }

    #[test]
    fn test_database_config_roundtrip() {
        let db_config = DatabaseConfig {
            url_template: "postgres:///{{db_name}}".to_string(),
            setup_command: Some("cargo sqlx database setup".to_string()),
            env_var: Some("DB_URL".to_string()),
        };
        let config = Config {
            projects: {
                let mut m = BTreeMap::new();
                m.insert(
                    "myproject".to_string(),
                    Project {
                        path: PathBuf::from("/tmp/myproject"),
                        worktree_base: None,
                        database: Some(db_config.clone()),
                    },
                );
                m
            },
        };
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        let project = deserialized.projects.get("myproject").unwrap();
        assert_eq!(project.database.as_ref(), Some(&db_config));
    }
}
