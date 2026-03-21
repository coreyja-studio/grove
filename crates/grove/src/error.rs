use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not determine config directory")]
    NoConfigDir,

    #[error("Project '{0}' already exists")]
    ProjectExists(String),

    #[error("Project '{0}' not found")]
    ProjectNotFound(String),

    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("Path is not a git/jj repository: {0}")]
    NotVcsRepo(PathBuf),

    #[error("Path does not belong to any registered project: {0}")]
    NoProjectForPath(PathBuf),

    #[error("No project detected (not registered and no .grove/config.toml found)")]
    NoProjectDetected,

    #[error("Invalid KEY=value format: {0}")]
    InvalidEnvFormat(String),

    #[error("VCS command failed: {0}")]
    VcsCommandFailed(String),

    #[error("jj is not installed but this project uses jj workspaces (.jj directory found). Install jj or use --vcs git to force git mode")]
    JjNotInstalled,

    #[error("Unknown VCS backend '{0}'. Supported: git")]
    InvalidVcsOverride(String),

    #[error("Worktree path already exists: {0}")]
    WorktreePathExists(PathBuf),

    #[error("Worktree '{0}' not found")]
    WorktreeNotFound(String),

    #[error("Invalid worktree name '{0}': must contain only alphanumeric characters, hyphens, and underscores")]
    InvalidWorktreeName(String),

    #[error("Ambiguous worktree name '{0}', could match: {1}")]
    AmbiguousWorktreeName(String, String),

    #[error("Invalid project reference '{0}': expected 'project' or 'project/worktree'")]
    InvalidProjectRef(String),

    #[error("Worktree '{1}' not found in project '{0}'")]
    WorktreeEnvNotFound(String, String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    TomlDeserialize(#[from] toml::de::Error),

    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error("Could not determine data directory for mise plugin installation")]
    NoDataDir,

    #[error("Failed to create database: {0}")]
    DatabaseCreationFailed(String),

    #[error("Failed to drop database: {0}")]
    DatabaseDropFailed(String),

    #[error("Setup command failed: {0}")]
    SetupCommandFailed(String),

    #[error("Post-create hook failed: `{0}`\n{1}")]
    HookFailed(String, String),

    #[error("Editor '{0}' exited with status {1}")]
    EditorFailed(String, String),
}
