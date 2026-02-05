use std::path::PathBuf;

use clap::Parser;

mod config;
mod error;

use config::{Config, EnvVars, ProjectRef};
use error::{Error, Result};

#[derive(Parser)]
#[command(name = "grove", version, about = "Manage a grove of git repositories")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Register an existing git repo
    Add {
        /// Project name
        name: String,
        /// Path to the git repository
        path: PathBuf,
    },
    /// Show all registered projects
    List,
    /// Unregister a project (doesn't delete files)
    Remove {
        /// Project name
        name: String,
    },
    /// Manage environment variables
    Env {
        #[command(subcommand)]
        command: EnvCommands,
    },
    /// Manage git worktrees
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommands,
    },
}

#[derive(clap::Subcommand)]
enum EnvCommands {
    /// Set an environment variable
    Set {
        /// Project name or project/worktree
        project: String,
        /// KEY=value pair
        pair: String,
    },
    /// Show all environment variables
    List {
        /// Project name or project/worktree
        project: String,
    },
    /// Remove an environment variable
    Unset {
        /// Project name or project/worktree
        project: String,
        /// Environment variable key to remove
        key: String,
    },
    /// Output environment variables for the project containing a path
    Export {
        /// Directory path
        path: PathBuf,
    },
}

#[derive(clap::Subcommand)]
enum WorktreeCommands {
    /// Create a new worktree
    New {
        /// Project name (must be registered)
        project: String,
        /// Worktree name (becomes branch name and directory suffix)
        name: String,
    },
    /// List worktrees
    List {
        /// Optional: filter to specific project
        project: Option<String>,
    },
    /// Remove a worktree
    Rm {
        /// Worktree name (full name like "project-feature" or just "feature" if unambiguous)
        name: String,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli.command) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Add { name, path } => cmd_add(&name, path),
        Commands::List => cmd_list(),
        Commands::Remove { name } => cmd_remove(&name),
        Commands::Env { command } => match command {
            EnvCommands::Set { project, pair } => cmd_env_set(&project, &pair),
            EnvCommands::List { project } => cmd_env_list(&project),
            EnvCommands::Unset { project, key } => cmd_env_unset(&project, &key),
            EnvCommands::Export { path } => cmd_env_export(path),
        },
        Commands::Worktree { command } => match command {
            WorktreeCommands::New { project, name } => cmd_worktree_new(&project, &name),
            WorktreeCommands::List { project } => cmd_worktree_list(project.as_deref()),
            WorktreeCommands::Rm { name } => cmd_worktree_rm(&name),
        },
    }
}

fn cmd_add(name: &str, path: PathBuf) -> Result<()> {
    let mut config = Config::load()?;
    config.add_project(name.to_string(), path)?;
    config.save()?;
    println!("Added project '{name}'");
    Ok(())
}

fn cmd_list() -> Result<()> {
    let config = Config::load()?;
    if config.projects.is_empty() {
        println!("No projects registered");
        return Ok(());
    }
    for (name, project) in &config.projects {
        println!("{name}\t{}", project.path.display());
    }
    Ok(())
}

fn cmd_remove(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.remove_project(name)?;
    config.save()?;
    println!("Removed project '{name}'");
    Ok(())
}

fn cmd_env_set(project: &str, pair: &str) -> Result<()> {
    let project_ref = ProjectRef::parse(project)?;

    let config = Config::load()?;
    let proj = config
        .projects
        .get(&project_ref.project)
        .ok_or_else(|| Error::ProjectNotFound(project_ref.project.clone()))?;

    let (key, value) = pair
        .split_once('=')
        .ok_or_else(|| Error::InvalidEnvFormat(pair.to_string()))?;

    if let Some(wt_name) = &project_ref.worktree {
        validate_worktree_exists(proj, &project_ref.project, wt_name)?;

        let mut vars = EnvVars::load_worktree(&project_ref.project, wt_name)?;
        vars.set(key.to_string(), value.to_string());
        vars.save_worktree(&project_ref.project, wt_name)?;
        println!("Set {key} for worktree '{project}'");
    } else {
        let mut vars = EnvVars::load(&project_ref.project)?;
        vars.set(key.to_string(), value.to_string());
        vars.save(&project_ref.project)?;
        println!("Set {key} for project '{}'", project_ref.project);
    }

    Ok(())
}

fn cmd_env_unset(project: &str, key: &str) -> Result<()> {
    let project_ref = ProjectRef::parse(project)?;

    let config = Config::load()?;
    let proj = config
        .projects
        .get(&project_ref.project)
        .ok_or_else(|| Error::ProjectNotFound(project_ref.project.clone()))?;

    if let Some(wt_name) = &project_ref.worktree {
        validate_worktree_exists(proj, &project_ref.project, wt_name)?;

        let mut vars = EnvVars::load_worktree(&project_ref.project, wt_name)?;
        if vars.remove(key) {
            if vars.vars.is_empty() {
                let path = config::worktree_env_path(&project_ref.project, wt_name)?;
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            } else {
                vars.save_worktree(&project_ref.project, wt_name)?;
            }
            println!("Unset {key} for worktree '{project}'");
        } else {
            println!("Key '{key}' not found in worktree '{project}'");
        }
    } else {
        let mut vars = EnvVars::load(&project_ref.project)?;
        if vars.remove(key) {
            if vars.vars.is_empty() {
                let path = config::env_path(&project_ref.project)?;
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            } else {
                vars.save(&project_ref.project)?;
            }
            println!("Unset {key} for project '{}'", project_ref.project);
        } else {
            println!("Key '{key}' not found in project '{}'", project_ref.project);
        }
    }

    Ok(())
}

fn cmd_env_list(project: &str) -> Result<()> {
    let project_ref = ProjectRef::parse(project)?;

    let config = Config::load()?;
    let proj = config
        .projects
        .get(&project_ref.project)
        .ok_or_else(|| Error::ProjectNotFound(project_ref.project.clone()))?;

    if let Some(wt_name) = &project_ref.worktree {
        validate_worktree_exists(proj, &project_ref.project, wt_name)?;

        let merged = config::load_merged_env(&project_ref)?;
        if merged.is_empty() {
            println!("No environment variables set for '{project}'");
            return Ok(());
        }

        // Calculate max key length for alignment
        let max_key_len = merged.iter().map(|v| v.key.len()).max().unwrap_or(0);

        for var in &merged {
            let source_label = match var.source {
                config::EnvSource::Worktree => "(override)",
                config::EnvSource::Project => "(from project)",
            };
            println!(
                "{:width$} = {}  {}",
                var.key,
                var.value,
                source_label,
                width = max_key_len
            );
        }
    } else {
        // Project-only: use existing code path unchanged for backward compatibility
        let vars = EnvVars::load(&project_ref.project)?;
        if vars.vars.is_empty() {
            println!("No environment variables set for '{}'", project_ref.project);
            return Ok(());
        }
        for (key, value) in &vars.vars {
            println!("{key}={value}");
        }
    }

    Ok(())
}

fn cmd_env_export(path: PathBuf) -> Result<()> {
    let config = Config::load()?;
    let project_ref = config
        .find_project_for_path(&path)?
        .ok_or(Error::NoProjectForPath(path))?;

    let merged = config::load_merged_env(&project_ref)?;
    let output = config::export_merged_env(&merged);
    if !output.is_empty() {
        println!("{output}");
    }
    Ok(())
}

/// Validate that a worktree actually exists for a project.
fn validate_worktree_exists(
    project: &config::Project,
    project_name: &str,
    worktree_name: &str,
) -> Result<()> {
    let worktrees = project.list_worktrees()?;
    let exists = worktrees.iter().any(|wt| {
        wt.path
            .file_name()
            .is_some_and(|n| n.to_string_lossy() == worktree_name)
    });
    if !exists {
        return Err(Error::WorktreeEnvNotFound(
            project_name.to_string(),
            worktree_name.to_string(),
        ));
    }
    Ok(())
}

/// Validate worktree name contains only alphanumeric, hyphens, and underscores.
fn validate_worktree_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::InvalidWorktreeName(name.to_string()));
    }
    Ok(())
}

fn cmd_worktree_new(project_name: &str, worktree_name: &str) -> Result<()> {
    validate_worktree_name(worktree_name)?;

    let config = Config::load()?;
    let project = config
        .projects
        .get(project_name)
        .ok_or_else(|| Error::ProjectNotFound(project_name.to_string()))?;

    let worktree_base = project.worktree_base();
    let worktree_path = worktree_base.join(worktree_name);

    if worktree_path.exists() {
        return Err(Error::WorktreePathExists(worktree_path));
    }

    // Create the worktree base directory if needed
    if !worktree_base.exists() {
        std::fs::create_dir_all(&worktree_base)?;
    }

    // Try to create worktree with new branch first
    let output = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            worktree_name,
        ])
        .current_dir(&project.path)
        .output()?;

    if !output.status.success() {
        // If branch already exists, try without -b
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
            let output2 = std::process::Command::new("git")
                .args([
                    "worktree",
                    "add",
                    worktree_path.to_str().unwrap(),
                    worktree_name,
                ])
                .current_dir(&project.path)
                .output()?;

            if !output2.status.success() {
                return Err(Error::GitCommandFailed(
                    String::from_utf8_lossy(&output2.stderr).to_string(),
                ));
            }
        } else {
            return Err(Error::GitCommandFailed(stderr.to_string()));
        }
    }

    println!("Created worktree at {}", worktree_path.display());
    Ok(())
}

fn cmd_worktree_list(project_filter: Option<&str>) -> Result<()> {
    let config = Config::load()?;

    // If project specified, verify it exists
    if let Some(name) = project_filter {
        if !config.projects.contains_key(name) {
            return Err(Error::ProjectNotFound(name.to_string()));
        }
    }

    let mut found_any = false;

    for (project_name, project) in &config.projects {
        if let Some(filter) = project_filter {
            if project_name != filter {
                continue;
            }
        }

        let worktrees = project.list_worktrees()?;
        for wt in worktrees {
            found_any = true;
            let dir_name = wt
                .path
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            let branch = wt.branch.as_deref().unwrap_or("(detached)");
            println!("{project_name}-{dir_name}\t{branch}\t{}", wt.path.display());
        }
    }

    if !found_any {
        if let Some(name) = project_filter {
            println!("No worktrees found for project '{name}'");
        } else {
            println!("No worktrees found");
        }
    }

    Ok(())
}

fn cmd_worktree_rm(name: &str) -> Result<()> {
    let config = Config::load()?;

    // Collect all worktrees across all projects
    let mut matches: Vec<(&str, config::Worktree)> = Vec::new();

    for (project_name, project) in &config.projects {
        let worktrees = project.list_worktrees()?;
        for wt in worktrees {
            let dir_name = wt
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            // Check for exact full name match (project-dirname)
            let full_name = format!("{project_name}-{dir_name}");
            if full_name == name {
                // Exact match - use this one
                return remove_worktree(project, &wt.path);
            }

            // Check if dirname matches
            if dir_name == name {
                matches.push((project_name, wt));
            }
        }
    }

    match matches.len() {
        0 => Err(Error::WorktreeNotFound(name.to_string())),
        1 => {
            let (project_name, wt) = matches.remove(0);
            let project = config.projects.get(project_name).unwrap();
            remove_worktree(project, &wt.path)
        }
        _ => {
            let candidates: Vec<String> = matches
                .iter()
                .map(|(proj, wt)| {
                    let dir_name = wt
                        .path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    format!("{proj}-{dir_name}")
                })
                .collect();
            Err(Error::AmbiguousWorktreeName(
                name.to_string(),
                candidates.join(", "),
            ))
        }
    }
}

fn remove_worktree(project: &config::Project, worktree_path: &std::path::Path) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(["worktree", "remove", worktree_path.to_str().unwrap()])
        .current_dir(&project.path)
        .output()?;

    if !output.status.success() {
        return Err(Error::GitCommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    println!("Removed worktree at {}", worktree_path.display());
    Ok(())
}
