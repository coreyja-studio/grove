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

    #[error("Path is not a git repository: {0}")]
    NotGitRepo(PathBuf),

    #[error("Path does not belong to any registered project: {0}")]
    NoProjectForPath(PathBuf),

    #[error("Invalid KEY=value format: {0}")]
    InvalidEnvFormat(String),

    #[error("Git command failed: {0}")]
    GitCommandFailed(String),

    #[error("Worktree path already exists: {0}")]
    WorktreePathExists(PathBuf),

    #[error("Worktree '{0}' not found")]
    WorktreeNotFound(String),

    #[error("Invalid worktree name '{0}': must contain only alphanumeric characters, hyphens, and underscores")]
    InvalidWorktreeName(String),

    #[error("Ambiguous worktree name '{0}', could match: {1}")]
    AmbiguousWorktreeName(String, String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    TomlDeserialize(#[from] toml::de::Error),

    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
}
