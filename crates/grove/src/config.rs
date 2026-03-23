//! Configuration management for grove.
//!
//! Grove uses a layered configuration system:
//!
//! 1. **Global config** (`~/.config/grove/config.toml`) — The project registry.
//!    Maps project names to repository paths, with optional database and hooks config.
//!
//! 2. **Repo config** (`.grove/config.toml` in the repository root) — Per-repo
//!    defaults for database URLs, hooks, and environment variables. Committed to
//!    the repo so all contributors share the same base config.
//!
//! 3. **Environment variables** (`~/.config/grove/envs/`) — Per-project and
//!    per-worktree env var overrides stored outside the repo. Worktree values
//!    override project values, which override repo defaults.

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub post_create: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub name: Option<String>,
    #[serde(default)]
    pub database: Option<DatabaseConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,
}

impl RepoConfig {
    pub fn load_from_dir(dir: &Path) -> Result<Option<Self>> {
        let config_path = dir.join(".grove").join("config.toml");
        if !config_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&config_path)?;
        let config: RepoConfig = toml::from_str(&content)?;
        Ok(Some(config))
    }

    /// Walk up from `path` looking for `.grove/config.toml`.
    /// Returns `(config, repo_root)` where the second element is the main repo root.
    /// When `.git` is a file (git worktree) or `.jj/repo` is a symlink (jj workspace),
    /// resolves back to the main repo root.
    pub fn discover(path: &Path) -> Result<Option<(Self, PathBuf)>> {
        let mut current = path.canonicalize()?;
        loop {
            if let Some(config) = Self::load_from_dir(&current)? {
                let dot_git = current.join(".git");
                if dot_git.is_file() {
                    // Git worktree — resolve .git file to find main repo
                    if let Some(main_repo) = resolve_main_repo_from_dot_git_file(&dot_git)? {
                        return Ok(Some((config, main_repo)));
                    }
                    // Resolution failed — fall through to continue walking up
                } else {
                    // Check if this is a jj workspace (not the main repo)
                    let dot_jj_repo = current.join(".jj").join("repo");
                    if dot_jj_repo.is_symlink() {
                        if let Some(main_repo) = resolve_main_repo_from_jj_workspace(&dot_jj_repo) {
                            return Ok(Some((config, main_repo)));
                        }
                    }
                    return Ok(Some((config, current)));
                }
            }
            if !current.pop() {
                return Ok(None);
            }
        }
    }

    pub fn effective_name(&self, repo_root: &Path) -> String {
        self.name.clone().unwrap_or_else(|| {
            repo_root.file_name().map_or_else(
                || "unknown".to_string(),
                |n| n.to_string_lossy().to_string(),
            )
        })
    }
}

/// Resolve a git worktree's `.git` file to find the main repository root.
///
/// In a git worktree, `.git` is a file containing `gitdir: <path>`, where
/// `<path>` points to `<main_repo>/.git/worktrees/<name>`. Walks up the gitdir
/// path to find the `.git` component and returns its parent as the main repo root.
fn resolve_main_repo_from_dot_git_file(dot_git_file: &Path) -> Result<Option<PathBuf>> {
    let content = fs::read_to_string(dot_git_file)?;
    let Some(gitdir_str) = content.trim().strip_prefix("gitdir: ") else {
        return Ok(None);
    };

    let base_dir = dot_git_file
        .parent()
        .expect("dot_git_file always has a parent directory");
    let gitdir_path = if Path::new(gitdir_str).is_absolute() {
        PathBuf::from(gitdir_str)
    } else {
        base_dir.join(gitdir_str)
    };

    let Ok(canonical) = gitdir_path.canonicalize() else {
        return Ok(None);
    };

    for ancestor in canonical.ancestors() {
        if ancestor.file_name() == Some(std::ffi::OsStr::new(".git")) {
            return Ok(ancestor.parent().map(Path::to_path_buf));
        }
    }

    Ok(None)
}

/// Resolve a jj workspace's `.jj/repo` symlink to find the main repository root.
///
/// In a jj workspace, `.jj/repo` is a symlink pointing to `<main_repo>/.jj/repo`.
fn resolve_main_repo_from_jj_workspace(dot_jj_repo: &Path) -> Option<PathBuf> {
    let canonical = dot_jj_repo.canonicalize().ok()?;
    // canonical points to <main_repo>/.jj/repo
    let jj_dir = canonical.parent()?;
    if jj_dir.file_name() == Some(std::ffi::OsStr::new(".jj")) {
        jj_dir.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

pub fn merge_project(
    repo_config: Option<&RepoConfig>,
    user_project: Option<&Project>,
    path: PathBuf,
) -> Project {
    let repo_db = repo_config.and_then(|rc| rc.database.clone());
    let repo_hooks = repo_config.and_then(|rc| rc.hooks.clone());

    Project {
        path,
        worktree_base: user_project.and_then(|p| p.worktree_base.clone()),
        database: user_project.and_then(|p| p.database.clone()).or(repo_db),
        hooks: user_project.and_then(|p| p.hooks.clone()).or(repo_hooks),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub path: PathBuf,
    #[serde(default)]
    pub worktree_base: Option<PathBuf>,
    #[serde(default)]
    pub database: Option<DatabaseConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
}

impl Project {
    /// Get the worktree base directory for this project.
    /// Returns the configured `worktree_base`, or defaults to `<project_path>/.worktrees`.
    pub fn worktree_base(&self) -> PathBuf {
        self.worktree_base
            .clone()
            .unwrap_or_else(|| self.path.join(".worktrees"))
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

        // Validate path exists and is a git/jj repo
        if !path.exists() {
            return Err(Error::PathNotFound(path));
        }
        if !path.join(".git").exists() && !path.join(".jj").exists() {
            return Err(Error::NotVcsRepo(path));
        }

        let canonical = path.canonicalize()?;
        self.projects.insert(
            name,
            Project {
                path: canonical,
                worktree_base: None,
                database: None,
                hooks: None,
            },
        );
        Ok(())
    }

    /// Register a discovered project to the registry, printing a message to stderr.
    /// Returns `true` if the project was newly registered, `false` if it was already present.
    /// On save failure, prints a warning to stderr and returns `false`.
    ///
    /// Unlike [`add_project`], this skips VCS validation and path canonicalization
    /// because `discover()` has already verified and canonicalized the path.
    pub fn register_discovered(&mut self, name: &str, project: Project) -> bool {
        if self.projects.contains_key(name) {
            return false;
        }
        self.projects.insert(name.to_string(), project);
        match self.save() {
            Ok(()) => {
                eprintln!("Registered \"{name}\" to project registry");
                true
            }
            Err(e) => {
                // Remove the entry we just inserted since save failed
                self.projects.remove(name);
                eprintln!("Warning: could not register \"{name}\" to project registry: {e}");
                false
            }
        }
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
    /// 1. Path is a subdirectory of a project's worktree base (more specific match)
    /// 2. Path is a subdirectory of a project's main repo
    pub fn find_project_for_path(&self, path: &Path) -> Result<Option<ProjectRef>> {
        let canonical = path.canonicalize()?;

        // First: check if path is in any project's worktree base (more specific match)
        for (name, project) in &self.projects {
            let wt_base = project.worktree_base();
            if let Ok(canonical_base) = wt_base.canonicalize() {
                if canonical.starts_with(&canonical_base) {
                    let rel = canonical.strip_prefix(&canonical_base).unwrap();
                    if let Some(wt_dir) = rel.components().next() {
                        let wt_name = wt_dir.as_os_str().to_string_lossy().to_string();
                        return Ok(Some(ProjectRef {
                            project: name.clone(),
                            worktree: Some(wt_name),
                        }));
                    }
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
    Repo,
    Project,
    Worktree,
}

#[derive(Debug)]
pub struct MergedEnvVar {
    pub key: String,
    pub value: String,
    pub source: EnvSource,
}

pub fn load_merged_env(
    project_name: &str,
    worktree: Option<&str>,
    repo_env: &BTreeMap<String, String>,
) -> Result<Vec<MergedEnvVar>> {
    // Layer 1: repo env vars (base)
    let mut merged: BTreeMap<String, (String, EnvSource)> = repo_env
        .iter()
        .map(|(k, v)| (k.clone(), (v.clone(), EnvSource::Repo)))
        .collect();

    // Layer 2: user project-level env vars
    let user_env = EnvVars::load(project_name)?;
    for (k, v) in user_env.vars {
        merged.insert(k, (v, EnvSource::Project));
    }

    // Layer 3: user worktree-level env vars
    if let Some(wt) = worktree {
        let wt_env = EnvVars::load_worktree(project_name, wt)?;
        for (k, v) in wt_env.vars {
            merged.insert(k, (v, EnvSource::Worktree));
        }
    }

    Ok(merged
        .into_iter()
        .map(|(key, (value, source))| MergedEnvVar { key, value, source })
        .collect())
}

/// Resolve a project by explicit name or auto-detection from cwd.
/// Returns `(name, project, repo_env_vars)`.
pub fn resolve_project(
    config: &mut Config,
    explicit_name: Option<&str>,
) -> Result<(String, Project, BTreeMap<String, String>)> {
    if let Some(name) = explicit_name {
        if let Some(user_proj) = config.projects.get(name) {
            let repo_config = RepoConfig::load_from_dir(&user_proj.path)?;
            let merged = merge_project(
                repo_config.as_ref(),
                Some(user_proj),
                user_proj.path.clone(),
            );
            let repo_env = repo_config.and_then(|rc| rc.env).unwrap_or_default();
            return Ok((name.to_string(), merged, repo_env));
        }

        // Not registered — try auto-detection from cwd and match by name
        let cwd = std::env::current_dir()?;
        if let Some((repo_config, repo_root)) = RepoConfig::discover(&cwd)? {
            let detected_name = repo_config.effective_name(&repo_root);
            if detected_name == name {
                let user_proj = config.projects.get(&detected_name);
                let path = user_proj.map_or(repo_root, |p| p.path.clone());
                let merged = merge_project(Some(&repo_config), user_proj, path);
                let repo_env = repo_config.env.unwrap_or_default();

                if !config.projects.contains_key(name) {
                    config.register_discovered(name, merged.clone());
                }

                return Ok((name.to_string(), merged, repo_env));
            }
        }

        return Err(Error::ProjectNotFound(name.to_string()));
    }

    let cwd = std::env::current_dir()?;

    if let Some(project_ref) = config.find_project_for_path(&cwd)? {
        let user_proj = config.projects.get(&project_ref.project).unwrap();
        let repo_config = RepoConfig::load_from_dir(&user_proj.path)?;
        let merged = merge_project(
            repo_config.as_ref(),
            Some(user_proj),
            user_proj.path.clone(),
        );
        let repo_env = repo_config.and_then(|rc| rc.env).unwrap_or_default();
        return Ok((project_ref.project, merged, repo_env));
    }

    if let Some((repo_config, repo_root)) = RepoConfig::discover(&cwd)? {
        let name = repo_config.effective_name(&repo_root);
        let user_proj = config.projects.get(&name);
        let path = user_proj.map(|p| p.path.clone()).unwrap_or(repo_root);
        let merged = merge_project(Some(&repo_config), user_proj, path);
        let repo_env = repo_config.env.unwrap_or_default();

        if !config.projects.contains_key(&name) {
            config.register_discovered(&name, merged.clone());
        }

        return Ok((name, merged, repo_env));
    }

    Err(Error::NoProjectDetected)
}

/// Resolved project info for a filesystem path.
pub type ResolvedProjectForPath = (String, Project, Option<String>, BTreeMap<String, String>);

/// Resolve a project for a filesystem path (used by env export).
/// Returns `(name, project, worktree_name, repo_env_vars)`.
pub fn resolve_project_for_path(
    config: &mut Config,
    path: &Path,
) -> Result<Option<ResolvedProjectForPath>> {
    if let Some(project_ref) = config.find_project_for_path(path)? {
        let user_proj = config.projects.get(&project_ref.project).unwrap();
        let repo_config = RepoConfig::load_from_dir(&user_proj.path)?;
        let merged = merge_project(
            repo_config.as_ref(),
            Some(user_proj),
            user_proj.path.clone(),
        );
        let repo_env = repo_config.and_then(|rc| rc.env).unwrap_or_default();
        return Ok(Some((
            project_ref.project,
            merged,
            project_ref.worktree,
            repo_env,
        )));
    }

    if let Some((repo_config, repo_root)) = RepoConfig::discover(path)? {
        let name = repo_config.effective_name(&repo_root);
        let user_proj = config.projects.get(&name);
        let proj_path = user_proj.map_or_else(|| repo_root.clone(), |p| p.path.clone());
        let merged = merge_project(Some(&repo_config), user_proj, proj_path);

        let canonical = path.canonicalize()?;
        let worktree = {
            let wt_base = merged.worktree_base();
            if let Ok(canonical_base) = wt_base.canonicalize() {
                if canonical.starts_with(&canonical_base) {
                    let rel = canonical.strip_prefix(&canonical_base).unwrap();
                    rel.components()
                        .next()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                } else {
                    None
                }
            } else {
                None
            }
        };

        let repo_env = repo_config.env.unwrap_or_default();

        if !config.projects.contains_key(&name) {
            config.register_discovered(&name, merged.clone());
        }

        return Ok(Some((name, merged, worktree, repo_env)));
    }

    Ok(None)
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
                        hooks: None,
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

    #[test]
    fn test_hooks_config_deserialization() {
        let toml_str = r#"
[projects.myproject]
path = "/tmp/myproject"

[projects.myproject.hooks]
post_create = ["yarn install", "cargo fetch"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let project = config.projects.get("myproject").unwrap();
        let hooks = project.hooks.as_ref().unwrap();
        assert_eq!(hooks.post_create.len(), 2);
        assert_eq!(hooks.post_create[0], "yarn install");
        assert_eq!(hooks.post_create[1], "cargo fetch");
    }

    #[test]
    fn test_hooks_config_absent_backward_compat() {
        let toml_str = r#"
[projects.myproject]
path = "/tmp/myproject"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let project = config.projects.get("myproject").unwrap();
        assert!(project.hooks.is_none());
    }

    #[test]
    fn test_hooks_config_empty_list() {
        let toml_str = r#"
[projects.myproject]
path = "/tmp/myproject"

[projects.myproject.hooks]
post_create = []
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let project = config.projects.get("myproject").unwrap();
        assert_eq!(
            project.hooks.as_ref(),
            Some(&HooksConfig {
                post_create: vec![]
            })
        );
    }

    #[test]
    fn test_hooks_config_roundtrip() {
        let hooks = HooksConfig {
            post_create: vec!["yarn install".to_string(), "cargo fetch".to_string()],
        };
        let config = Config {
            projects: {
                let mut m = BTreeMap::new();
                m.insert(
                    "myproject".to_string(),
                    Project {
                        path: PathBuf::from("/tmp/myproject"),
                        worktree_base: None,
                        database: None,
                        hooks: Some(hooks.clone()),
                    },
                );
                m
            },
        };
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        let project = deserialized.projects.get("myproject").unwrap();
        assert_eq!(project.hooks.as_ref(), Some(&hooks));
    }

    #[test]
    fn test_repo_config_deserialization() {
        let toml_str = r#"
name = "mull"

[database]
url_template = "postgres:///{{db_name}}"
setup_command = "cargo sqlx database setup"

[hooks]
post_create = ["yarn install"]

[env]
RUST_LOG = "debug"
NODE_ENV = "development"
"#;
        let config: RepoConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.name.as_deref(), Some("mull"));
        assert!(config.database.is_some());
        assert_eq!(config.hooks.as_ref().unwrap().post_create.len(), 1);
        let env = config.env.as_ref().unwrap();
        assert_eq!(env.get("RUST_LOG").unwrap(), "debug");
    }

    #[test]
    fn test_repo_config_minimal() {
        let toml_str = "";
        let config: RepoConfig = toml::from_str(toml_str).unwrap();
        assert!(config.name.is_none());
        assert!(config.database.is_none());
        assert!(config.hooks.is_none());
        assert!(config.env.is_none());
    }

    #[test]
    fn test_repo_config_name_only() {
        let toml_str = r#"name = "myproject""#;
        let config: RepoConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.name.as_deref(), Some("myproject"));
    }

    #[test]
    fn test_effective_name_explicit() {
        let config = RepoConfig {
            name: Some("mull".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config.effective_name(Path::new("/home/user/code/my-repo")),
            "mull"
        );
    }

    #[test]
    fn test_effective_name_fallback_to_dir() {
        let config = RepoConfig::default();
        assert_eq!(
            config.effective_name(Path::new("/home/user/code/my-repo")),
            "my-repo"
        );
    }

    #[test]
    fn test_merge_project_repo_only() {
        let repo = RepoConfig {
            database: Some(DatabaseConfig {
                url_template: "postgres:///{{db_name}}".to_string(),
                setup_command: None,
                env_var: None,
            }),
            hooks: Some(HooksConfig {
                post_create: vec!["yarn install".to_string()],
            }),
            ..Default::default()
        };
        let merged = merge_project(Some(&repo), None, PathBuf::from("/tmp/repo"));
        assert_eq!(merged.path, PathBuf::from("/tmp/repo"));
        assert!(merged.database.is_some());
        assert!(merged.hooks.is_some());
        assert!(merged.worktree_base.is_none());
    }

    #[test]
    fn test_merge_project_user_overrides_repo() {
        let repo = RepoConfig {
            database: Some(DatabaseConfig {
                url_template: "postgres:///{{db_name}}".to_string(),
                setup_command: None,
                env_var: None,
            }),
            ..Default::default()
        };
        let user = Project {
            path: PathBuf::from("/tmp/repo"),
            worktree_base: Some(PathBuf::from("/tmp/worktrees")),
            database: Some(DatabaseConfig {
                url_template: "postgres://localhost/{{db_name}}".to_string(),
                setup_command: Some("migrate".to_string()),
                env_var: None,
            }),
            hooks: None,
        };
        let merged = merge_project(Some(&repo), Some(&user), user.path.clone());
        assert_eq!(
            merged.database.as_ref().unwrap().url_template,
            "postgres://localhost/{{db_name}}"
        );
        assert_eq!(merged.worktree_base, Some(PathBuf::from("/tmp/worktrees")));
        assert!(merged.hooks.is_none());
    }

    #[test]
    fn test_merge_project_repo_fills_gaps() {
        let repo = RepoConfig {
            hooks: Some(HooksConfig {
                post_create: vec!["setup.sh".to_string()],
            }),
            ..Default::default()
        };
        let user = Project {
            path: PathBuf::from("/tmp/repo"),
            worktree_base: None,
            database: None,
            hooks: None,
        };
        let merged = merge_project(Some(&repo), Some(&user), user.path.clone());
        assert!(merged.hooks.is_some());
        assert_eq!(merged.hooks.unwrap().post_create, vec!["setup.sh"]);
    }

    #[test]
    fn test_load_merged_env_repo_layer() {
        let repo_env: BTreeMap<String, String> =
            [("REPO_VAR".to_string(), "from_repo".to_string())]
                .into_iter()
                .collect();

        // EnvVars::load returns empty when no file exists (returns default).
        // The repo env layer should come through as the base.
        let merged = load_merged_env("nonexistent_test_project_12345", None, &repo_env).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, "REPO_VAR");
        assert_eq!(merged[0].value, "from_repo");
        assert!(matches!(merged[0].source, EnvSource::Repo));
    }

    #[test]
    fn test_load_merged_env_empty_layers() {
        let repo_env = BTreeMap::new();
        let merged = load_merged_env("nonexistent_test_project_12345", None, &repo_env).unwrap();
        assert!(merged.is_empty());
    }

    #[test]
    fn test_resolve_main_repo_from_dot_git_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_repo = tmp.path().join("main-repo");
        let worktree = tmp.path().join("worktrees").join("feature");
        let git_worktrees = main_repo.join(".git").join("worktrees").join("feature");

        fs::create_dir_all(&git_worktrees).unwrap();
        fs::create_dir_all(&worktree).unwrap();

        let gitdir_content = format!("gitdir: {}", git_worktrees.display());
        fs::write(worktree.join(".git"), &gitdir_content).unwrap();

        let result = resolve_main_repo_from_dot_git_file(&worktree.join(".git")).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), main_repo.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_main_repo_from_dot_git_file_relative() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_repo = tmp.path().join("repos").join("main");
        let worktree = main_repo.join(".worktrees").join("feature");
        let git_worktrees = main_repo.join(".git").join("worktrees").join("feature");

        fs::create_dir_all(&git_worktrees).unwrap();
        fs::create_dir_all(&worktree).unwrap();

        let gitdir_content = "gitdir: ../../.git/worktrees/feature";
        fs::write(worktree.join(".git"), gitdir_content).unwrap();

        let result = resolve_main_repo_from_dot_git_file(&worktree.join(".git")).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), main_repo.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_main_repo_invalid_format() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dot_git = tmp.path().join(".git");
        fs::write(&dot_git, "not a valid gitdir line").unwrap();

        let result = resolve_main_repo_from_dot_git_file(&dot_git).unwrap();
        assert!(result.is_none());
    }
}
