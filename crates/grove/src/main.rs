//! # Grove
//!
//! A lightweight CLI for managing multiple git/jj repositories with per-project
//! environment variables and centralized worktree management.
//!
//! ## Core Concepts
//!
//! - **Project Registry** — Track multiple repositories under short names.
//!   Register with `grove add`, list with `grove list`.
//! - **Environment Variables** — Layered env vars stored outside the repo.
//!   Project-level defaults, worktree-level overrides, and repo-level
//!   defaults in `.grove/config.toml`. Integrates with [mise](https://mise.jdx.dev/)
//!   for automatic shell injection.
//! - **Worktree Management** — Create and manage git worktrees (or jj workspaces)
//!   across all projects from a single command. Worktrees get their own
//!   database instances and env var overrides.
//!
//! ## Quick Start
//!
//! ```bash
//! # Register a project
//! grove add myapp /path/to/repo
//!
//! # Set environment variables
//! grove env set myapp DATABASE_URL=postgres:///myapp_dev
//!
//! # Create a worktree and start working
//! grove start myapp my-feature
//!
//! # List all worktrees across projects
//! grove worktree list
//! ```
//!
//! ## Modules
//!
//! - [`config`] — Configuration loading, project registry, env var management
//! - [`vcs`] — VCS backend abstraction (git, jj)
//! - [`error`] — Error types

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Parser;

mod config;
mod error;
mod vcs;

use config::{Config, EnvVars, ProjectRef};
use error::{Error, Result};

const MISE_METADATA_LUA: &str = include_str!("mise_plugin/metadata.lua");
const MISE_ENV_LUA: &str = include_str!("mise_plugin/mise_env.lua");

#[derive(Parser)]
#[command(
    name = "grove",
    version,
    about = "Manage a grove of git/jj repositories",
    long_about = "Manage a grove of git/jj repositories.\n\n\
        Grove tracks multiple git and jj repositories, manages isolated worktrees \
        for each, and provides per-project and per-worktree environment variables. \
        It integrates with mise to inject environment variables automatically.\n\n\
        Projects can be registered explicitly with `grove add` or auto-detected \
        from a .grove/config.toml file in the repository root. Worktrees are \
        created in a configurable base directory and can have their own database \
        instances and environment variable overrides.",
    after_help = "Get started:\n  \
        grove add myproject /path/to/repo\n  \
        grove start myproject my-feature\n\n\
        See `grove <command> --help` for detailed usage of each command."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Register an existing git repo
    #[command(
        long_about = "Register an existing git or jj repository with grove under a short name.\n\n\
            The path must point to a directory containing a .git or .jj directory. \
            The project name is used in all other grove commands to reference this \
            repository. Once registered, you can create worktrees, manage env vars, \
            and use `grove start` with this project.",
        after_help = "Examples:\n  \
            grove add myapp /home/user/code/myapp\n  \
            grove add api ~/projects/api-server"
    )]
    Add {
        /// Short name to identify this project in grove commands
        name: String,
        /// Path to the git or jj repository root (must contain .git or .jj)
        path: PathBuf,
    },
    /// Show all registered projects
    #[command(
        long_about = "Show all projects registered with grove.\n\n\
            Outputs each project's name and path, tab-separated. \
            Projects are registered with `grove add` and stored in \
            the grove config file (~/.config/grove/config.toml).",
        after_help = "Examples:\n  grove list"
    )]
    List,
    /// Unregister a project (doesn't delete files)
    #[command(
        long_about = "Unregister a project from grove.\n\n\
            This removes the project from grove's config but does NOT delete \
            the repository, worktrees, or any files on disk. To also clean up \
            worktrees, remove them first with `grove worktree rm`.",
        after_help = "Examples:\n  grove remove myapp"
    )]
    Remove {
        /// Name of the project to unregister
        name: String,
    },
    /// Manage environment variables
    #[command(
        long_about = "Manage per-project and per-worktree environment variables.\n\n\
            Environment variables are stored in ~/.config/grove/envs/ and can be \
            injected into your shell via the mise integration (`grove init-mise`). \
            Variables set at the project level apply to all worktrees. Variables \
            set at the worktree level override project-level values. Repo-level \
            defaults can also be defined in .grove/config.toml under [env].\n\n\
            When a project argument is omitted, grove auto-detects the project \
            from your current working directory.",
        after_help = "Examples:\n  \
            grove env set myapp DATABASE_URL=postgres://localhost/myapp\n  \
            grove env list myapp\n  \
            grove env unset myapp DATABASE_URL"
    )]
    Env {
        #[command(subcommand)]
        command: EnvCommands,
    },
    /// Manage git/jj worktrees
    #[command(
        long_about = "Manage git worktrees and jj workspaces for registered projects.\n\n\
            Grove creates worktrees in a configurable base directory (defaults to \
            <repo>/.worktrees/). Each worktree gets its own branch (git) or workspace \
            (jj). If the project has database configuration, a dedicated database is \
            created per worktree and the DATABASE_URL is set automatically.\n\n\
            The VCS backend is auto-detected: if the repo has a .jj directory, jj is \
            used; otherwise git. Use --vcs git to force git mode for colocated repos.",
        after_help = "Examples:\n  \
            grove worktree new myapp my-feature\n  \
            grove worktree list myapp\n  \
            grove worktree rm myapp-my-feature"
    )]
    Worktree {
        /// Force a specific VCS backend (e.g., "git") instead of auto-detection
        #[arg(long)]
        vcs: Option<String>,
        #[command(subcommand)]
        command: WorktreeCommands,
    },
    /// Create a worktree, run hooks, and open editor
    #[command(
        long_about = "Create a new worktree (or reuse an existing one), run lifecycle hooks, \
            and open it in your $EDITOR.\n\n\
            This is the high-level \"start working\" command. It combines worktree \
            creation, database provisioning, mise trust, post-create hooks, and editor \
            launch into a single step. If the worktree already exists, it skips creation \
            and just opens the editor.\n\n\
            Requires $EDITOR to be set for the editor launch (silently skipped if unset).",
        after_help = "Examples:\n  \
            grove start myapp my-feature\n  \
            grove start myapp bugfix-123 --vcs git"
    )]
    Start {
        /// Name of the registered project
        project: String,
        /// Name for the new worktree (alphanumeric, hyphens, underscores only)
        name: String,
        /// Force a specific VCS backend (e.g., "git") instead of auto-detection
        #[arg(long)]
        vcs: Option<String>,
    },
    /// Install grove plugin for mise
    #[command(
        long_about = "Install the grove plugin for mise so that grove-managed environment \
            variables are automatically injected into your shell.\n\n\
            This copies the plugin files to mise's plugin directory \
            (~/.local/share/mise/plugins/grove/) and prints the config snippet \
            you need to add to ~/.config/mise/config.toml.\n\n\
            After running this command and adding the config, mise will automatically \
            export grove environment variables when you cd into a project directory.",
        after_help = "Examples:\n  grove init-mise\n\n\
            After running, add to ~/.config/mise/config.toml:\n  \
            [env]\n  _.grove = {}"
    )]
    InitMise,
}

#[derive(clap::Subcommand)]
enum EnvCommands {
    /// Set an environment variable
    #[command(
        long_about = "Set an environment variable for a project or worktree.\n\n\
            The project can be specified explicitly or auto-detected from your \
            current directory. Use the project/worktree syntax to set a variable \
            on a specific worktree (overrides the project-level value).",
        after_help = "Examples:\n  \
            # Set for a project (explicit name):\n  \
            grove env set myapp DATABASE_URL=postgres://localhost/myapp\n\n  \
            # Set for current project (auto-detected from cwd):\n  \
            grove env set SECRET_KEY=abc123\n\n  \
            # Set for a specific worktree:\n  \
            grove env set myapp/my-feature DATABASE_URL=postgres://localhost/myapp_feature"
    )]
    Set {
        /// Project name or KEY=value pair (auto-detects project from cwd if KEY=value)
        project_or_pair: String,
        /// KEY=value pair (when first argument is a project name)
        pair: Option<String>,
    },
    /// Show all environment variables
    #[command(
        long_about = "List all environment variables for a project or worktree.\n\n\
            Shows variables from all sources (repo .grove/config.toml, project-level, \
            and worktree-level) with labels indicating where each value comes from. \
            Worktree-level values override project-level values, which override repo-level.\n\n\
            If no project is specified, auto-detects from the current directory.",
        after_help = "Examples:\n  \
            grove env list myapp\n  \
            grove env list myapp/my-feature\n  \
            grove env list          # auto-detect from cwd"
    )]
    List {
        /// Project name or project/worktree (auto-detects from cwd if omitted)
        project: Option<String>,
    },
    /// Remove an environment variable
    #[command(
        long_about = "Remove an environment variable from a project or worktree.\n\n\
            The project can be specified explicitly or auto-detected from the \
            current directory. Use project/worktree syntax to remove a worktree-level \
            override (the project-level value will then apply again).",
        after_help = "Examples:\n  \
            grove env unset myapp SECRET_KEY\n  \
            grove env unset SECRET_KEY           # auto-detect project from cwd\n  \
            grove env unset myapp/feat DATABASE_URL"
    )]
    Unset {
        /// Project name or env var key (auto-detects project from cwd if only one arg)
        project_or_key: String,
        /// Env var key (when first argument is a project name)
        key: Option<String>,
    },
    /// Output environment variables for the project containing a path
    #[command(
        long_about = "Export resolved environment variables for a given directory path.\n\n\
            Looks up which project (and optionally worktree) owns the given path, \
            merges all env var layers (repo, project, worktree), and outputs them. \
            By default outputs in KEY=value format (one per line); with --json \
            outputs a JSON object.\n\n\
            This is primarily used by the mise plugin to inject variables into the \
            shell, but can also be used for scripting.",
        after_help = "Examples:\n  \
            grove env export /home/user/code/myapp\n  \
            grove env export --json /home/user/code/myapp/.worktrees/feat\n  \
            eval \"$(grove env export /home/user/code/myapp)\""
    )]
    Export {
        /// Output as a JSON object instead of KEY=value lines
        #[arg(long)]
        json: bool,
        /// Directory path to resolve the project and worktree from
        path: PathBuf,
    },
}

#[derive(clap::Subcommand)]
enum WorktreeCommands {
    /// Create a new worktree
    #[command(
        long_about = "Create a new git worktree or jj workspace for a project.\n\n\
            The worktree is created in the project's worktree base directory \
            (defaults to <repo>/.worktrees/<name>). For git repos, a new branch \
            with the worktree name is created. For jj repos, a new workspace is added.\n\n\
            If the project has database configuration in .grove/config.toml, a \
            dedicated database is created and DATABASE_URL is set automatically. \
            Post-create hooks (if configured) run after creation.\n\n\
            The project can be specified explicitly or auto-detected from cwd.",
        after_help = "Examples:\n  \
            # Explicit project:\n  \
            grove worktree new myapp my-feature\n\n  \
            # Auto-detect project from cwd:\n  \
            grove worktree new my-feature"
    )]
    New {
        /// Worktree name, or project name if second argument is provided
        name_or_project: String,
        /// Worktree name (when provided, first argument is the project name)
        name: Option<String>,
    },
    /// List worktrees
    #[command(
        long_about = "List all worktrees across registered projects.\n\n\
            Shows each worktree's full name (project-worktree), branch, and path, \
            tab-separated. If a project is specified, only shows worktrees for \
            that project. Also includes worktrees from auto-detected projects \
            (via .grove/config.toml in cwd).",
        after_help = "Examples:\n  \
            grove worktree list\n  \
            grove worktree list myapp"
    )]
    List {
        /// Only show worktrees for this project
        project: Option<String>,
    },
    /// Remove a worktree
    #[command(
        long_about = "Remove a git worktree or jj workspace.\n\n\
            Accepts either the full name (e.g., \"myapp-my-feature\") or just the \
            short worktree name (e.g., \"my-feature\") if it's unambiguous across \
            all projects. If the project has database configuration, the worktree's \
            database is dropped automatically.\n\n\
            For git, this runs `git worktree remove`. For jj, this runs \
            `jj workspace forget` and deletes the directory.",
        after_help = "Examples:\n  \
            grove worktree rm myapp-my-feature\n  \
            grove worktree rm my-feature          # if unambiguous"
    )]
    Rm {
        /// Worktree name (full \"project-name\" or short \"name\" if unambiguous)
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
            EnvCommands::Set {
                project_or_pair,
                pair,
            } => cmd_env_set(&project_or_pair, pair.as_deref()),
            EnvCommands::List { project } => cmd_env_list(project.as_deref()),
            EnvCommands::Unset {
                project_or_key,
                key,
            } => cmd_env_unset(&project_or_key, key.as_deref()),
            EnvCommands::Export { json, path } => cmd_env_export(path, json),
        },
        Commands::InitMise => cmd_init_mise(),
        Commands::Start { project, name, vcs } => {
            let vcs_override = parse_vcs_override(vcs.as_deref())?;
            cmd_start(&project, &name, vcs_override)
        }
        Commands::Worktree { vcs, command } => {
            let vcs_override = parse_vcs_override(vcs.as_deref())?;
            match command {
                WorktreeCommands::New {
                    name_or_project,
                    name,
                } => cmd_worktree_new(&name_or_project, name.as_deref(), vcs_override),
                WorktreeCommands::List { project } => {
                    cmd_worktree_list(project.as_deref(), vcs_override)
                }
                WorktreeCommands::Rm { name } => cmd_worktree_rm(&name, vcs_override),
            }
        }
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

fn cmd_env_set(project_or_pair: &str, pair: Option<&str>) -> Result<()> {
    let (project_str, pair_str) = match pair {
        Some(p) => (Some(project_or_pair), p),
        None => (None, project_or_pair),
    };

    let config = Config::load()?;
    // Resolve project once — either from explicit name or auto-detection.
    // For the two-arg form, project_ref may include a worktree specifier (project/worktree).
    let (project_ref, resolved) = if let Some(s) = project_str {
        let pr = ProjectRef::parse(s)?;
        let resolved = config::resolve_project(&config, Some(&pr.project))?;
        (pr, resolved)
    } else {
        let (name, project, repo_env) = config::resolve_project(&config, None)?;
        let pr = ProjectRef {
            project: name.clone(),
            worktree: None,
        };
        (pr, (name, project, repo_env))
    };

    let (key, value) = pair_str
        .split_once('=')
        .ok_or_else(|| Error::InvalidEnvFormat(pair_str.to_string()))?;

    if let Some(wt_name) = &project_ref.worktree {
        validate_worktree_exists(&resolved.1, &project_ref.project, wt_name)?;

        let mut vars = EnvVars::load_worktree(&project_ref.project, wt_name)?;
        vars.set(key.to_string(), value.to_string());
        vars.save_worktree(&project_ref.project, wt_name)?;
        println!("Set {key} for worktree '{}/{wt_name}'", project_ref.project);
    } else {
        let mut vars = EnvVars::load(&project_ref.project)?;
        vars.set(key.to_string(), value.to_string());
        vars.save(&project_ref.project)?;
        println!("Set {key} for project '{}'", project_ref.project);
    }

    Ok(())
}

fn cmd_env_unset(project_or_key: &str, key: Option<&str>) -> Result<()> {
    let (project_str, actual_key) = match key {
        Some(k) => (Some(project_or_key), k),
        None => (None, project_or_key),
    };

    let config = Config::load()?;
    let project_ref = if let Some(s) = project_str {
        ProjectRef::parse(s)?
    } else {
        let (name, _, _) = config::resolve_project(&config, None)?;
        ProjectRef {
            project: name,
            worktree: None,
        }
    };

    let (_, resolved_project, _) = config::resolve_project(&config, Some(&project_ref.project))?;

    if let Some(wt_name) = &project_ref.worktree {
        validate_worktree_exists(&resolved_project, &project_ref.project, wt_name)?;

        let mut vars = EnvVars::load_worktree(&project_ref.project, wt_name)?;
        if vars.remove(actual_key) {
            if vars.vars.is_empty() {
                let path = config::worktree_env_path(&project_ref.project, wt_name)?;
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            } else {
                vars.save_worktree(&project_ref.project, wt_name)?;
            }
            println!(
                "Unset {actual_key} for worktree '{}/{wt_name}'",
                project_ref.project
            );
        } else {
            println!(
                "Key '{actual_key}' not found in worktree '{}/{wt_name}'",
                project_ref.project
            );
        }
    } else {
        let mut vars = EnvVars::load(&project_ref.project)?;
        if vars.remove(actual_key) {
            if vars.vars.is_empty() {
                let path = config::env_path(&project_ref.project)?;
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            } else {
                vars.save(&project_ref.project)?;
            }
            println!("Unset {actual_key} for project '{}'", project_ref.project);
        } else {
            println!(
                "Key '{actual_key}' not found in project '{}'",
                project_ref.project
            );
        }
    }

    Ok(())
}

fn cmd_env_list(project: Option<&str>) -> Result<()> {
    let config = Config::load()?;

    let project_ref = if let Some(s) = project {
        ProjectRef::parse(s)?
    } else {
        let (name, _, _) = config::resolve_project(&config, None)?;
        ProjectRef {
            project: name,
            worktree: None,
        }
    };

    let (_, resolved_project, repo_env) =
        config::resolve_project(&config, Some(&project_ref.project))?;

    if let Some(wt_name) = &project_ref.worktree {
        validate_worktree_exists(&resolved_project, &project_ref.project, wt_name)?;

        let merged = config::load_merged_env(&project_ref.project, Some(wt_name), &repo_env)?;
        if merged.is_empty() {
            println!(
                "No environment variables set for '{}/{wt_name}'",
                project_ref.project
            );
            return Ok(());
        }

        let max_key_len = merged.iter().map(|v| v.key.len()).max().unwrap_or(0);
        for var in &merged {
            let source_label = match var.source {
                config::EnvSource::Repo => "(from repo)",
                config::EnvSource::Project => "(from project)",
                config::EnvSource::Worktree => "(override)",
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
        let merged = config::load_merged_env(&project_ref.project, None, &repo_env)?;
        if merged.is_empty() {
            println!("No environment variables set for '{}'", project_ref.project);
            return Ok(());
        }

        if repo_env.is_empty() {
            // Backward compat: no repo env, use plain KEY=value format
            for var in &merged {
                println!("{}={}", var.key, var.value);
            }
        } else {
            let max_key_len = merged.iter().map(|v| v.key.len()).max().unwrap_or(0);
            for var in &merged {
                let source_label = match var.source {
                    config::EnvSource::Repo => "(from repo)",
                    config::EnvSource::Project | config::EnvSource::Worktree => "(override)",
                };
                println!(
                    "{:width$} = {}  {}",
                    var.key,
                    var.value,
                    source_label,
                    width = max_key_len
                );
            }
        }
    }

    Ok(())
}

fn cmd_env_export(path: PathBuf, json: bool) -> Result<()> {
    if json {
        if !path.exists() {
            println!("{{}}");
            return Ok(());
        }

        let config = Config::load()?;
        let Some((name, _project, worktree, repo_env)) =
            config::resolve_project_for_path(&config, &path)?
        else {
            println!("{{}}");
            return Ok(());
        };

        let merged = config::load_merged_env(&name, worktree.as_deref(), &repo_env)?;
        let map: BTreeMap<String, String> = merged.into_iter().map(|v| (v.key, v.value)).collect();
        let json_str = serde_json::to_string(&map)?;
        println!("{json_str}");
    } else {
        let config = Config::load()?;
        let (name, _project, worktree, repo_env) =
            config::resolve_project_for_path(&config, &path)?
                .ok_or(Error::NoProjectForPath(path))?;

        let merged = config::load_merged_env(&name, worktree.as_deref(), &repo_env)?;
        let output = config::export_merged_env(&merged);
        if !output.is_empty() {
            println!("{output}");
        }
    }
    Ok(())
}

fn parse_vcs_override(vcs: Option<&str>) -> Result<Option<vcs::VcsOverride>> {
    match vcs.map(str::to_lowercase).as_deref() {
        None => Ok(None),
        Some("git") => Ok(Some(vcs::VcsOverride::Git)),
        Some(other) => Err(Error::InvalidVcsOverride(other.to_string())),
    }
}

/// Validate that a worktree actually exists for a project.
fn validate_worktree_exists(
    project: &config::Project,
    project_name: &str,
    worktree_name: &str,
) -> Result<()> {
    let backend = vcs::detect_backend(&project.path, None)?;
    let worktrees = backend.list_worktrees(&project.path, &project.worktree_base())?;
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

fn create_database(db_name: &str) -> Result<()> {
    let output = std::process::Command::new("createdb")
        .arg(db_name)
        .output()?;

    if !output.status.success() {
        return Err(Error::DatabaseCreationFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

fn drop_database(db_name: &str) -> Result<()> {
    let output = std::process::Command::new("dropdb")
        .args(["--if-exists", db_name])
        .output()?;

    if !output.status.success() {
        return Err(Error::DatabaseDropFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

fn run_setup_command(
    command: &str,
    worktree_path: &std::path::Path,
    env_var_name: &str,
    database_url: &str,
) -> Result<()> {
    let output = std::process::Command::new("sh")
        .args(["-c", command])
        .current_dir(worktree_path)
        .env(env_var_name, database_url)
        .output()?;

    if !output.status.success() {
        return Err(Error::SetupCommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

fn run_mise_trust(worktree_path: &std::path::Path) -> Result<()> {
    let output = match std::process::Command::new("mise")
        .arg("trust")
        .current_dir(worktree_path)
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    if output.status.success() {
        println!("Ran mise trust");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: mise trust failed: {stderr}");
    }
    Ok(())
}

fn run_post_create_hooks(hooks: &[String], worktree_path: &std::path::Path) -> Result<()> {
    for cmd in hooks {
        println!("Running hook: {cmd}");
        let output = std::process::Command::new("sh")
            .args(["-c", cmd])
            .current_dir(worktree_path)
            .output()?;

        if !output.status.success() {
            return Err(Error::HookFailed(
                cmd.clone(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        println!("Hook completed: {cmd}");
    }
    Ok(())
}

fn create_worktree_with_hooks(
    project_name: &str,
    project: &config::Project,
    worktree_name: &str,
    vcs_override: Option<vcs::VcsOverride>,
) -> Result<std::path::PathBuf> {
    validate_worktree_name(worktree_name)?;

    let worktree_base = project.worktree_base();
    let worktree_path = worktree_base.join(worktree_name);

    if worktree_path.exists() {
        return Err(Error::WorktreePathExists(worktree_path));
    }

    if !worktree_base.exists() {
        std::fs::create_dir_all(&worktree_base)?;
    }

    let backend = vcs::detect_backend(&project.path, vcs_override)?;
    backend.create_worktree(&project.path, &worktree_path, worktree_name)?;

    println!("Created worktree at {}", worktree_path.display());

    if let Some(db_config) = &project.database {
        let db_name = db_config.db_name(project_name, worktree_name);
        println!("Creating database '{db_name}'...");
        create_database(&db_name)?;
        println!("Created database '{db_name}'");

        let db_url = db_config.database_url(project_name, worktree_name);
        let env_var = db_config.env_var_name();

        let mut env_vars = EnvVars::load_worktree(project_name, worktree_name)?;
        env_vars.set(env_var.to_string(), db_url.clone());
        env_vars.save_worktree(project_name, worktree_name)?;
        println!("Set {env_var} for worktree '{project_name}/{worktree_name}'");

        if let Some(cmd) = &db_config.setup_command {
            println!("Running setup command: {cmd}");
            run_setup_command(cmd, &worktree_path, env_var, &db_url)?;
            println!("Setup command completed");
        }
    }

    run_mise_trust(&worktree_path)?;

    if let Some(hooks) = &project.hooks {
        if !hooks.post_create.is_empty() {
            run_post_create_hooks(&hooks.post_create, &worktree_path)?;
        }
    }

    Ok(worktree_path)
}

/// Opens `$EDITOR` pointed at the given path, if `$EDITOR` is set.
///
/// Uses `sh -c` to support editors with arguments (e.g., `EDITOR="code --wait"`).
/// If `$EDITOR` is not set, returns `Ok(())` silently.
fn open_editor(path: &std::path::Path) -> Result<()> {
    if std::env::var_os("EDITOR").is_none() {
        return Ok(());
    }

    let status = std::process::Command::new("sh")
        .args(["-c", r#"$EDITOR "$@""#, "--", path.to_str().unwrap()])
        .status()?;

    if !status.success() {
        let editor = std::env::var("EDITOR").unwrap_or_default();
        let code = status
            .code()
            .map_or_else(|| "unknown".to_string(), |c| c.to_string());
        return Err(Error::EditorFailed(editor, code));
    }

    Ok(())
}

fn cmd_start(project: &str, name: &str, vcs_override: Option<vcs::VcsOverride>) -> Result<()> {
    validate_worktree_name(name)?;

    let config = Config::load()?;
    let (project_name, resolved_project, _repo_env) =
        config::resolve_project(&config, Some(project))?;

    let worktree_path = resolved_project.worktree_base().join(name);

    if worktree_path.exists()
        && (worktree_path.join(".git").exists() || worktree_path.join(".jj").exists())
    {
        // Valid existing worktree — reuse it
        eprintln!("worktree '{name}' already exists for {project_name}, reusing");
    } else {
        // Either doesn't exist, or is an orphaned directory without .git/.jj
        if worktree_path.exists() {
            // Orphaned directory — clean up before recreating
            std::fs::remove_dir_all(&worktree_path)?;
        }
        create_worktree_with_hooks(&project_name, &resolved_project, name, vcs_override)?;
    }

    open_editor(&worktree_path)?;
    Ok(())
}

fn cmd_worktree_new(
    name_or_project: &str,
    name: Option<&str>,
    vcs_override: Option<vcs::VcsOverride>,
) -> Result<()> {
    let (explicit_project, worktree_name) = match name {
        Some(wt_name) => (Some(name_or_project), wt_name),
        None => (None, name_or_project),
    };

    let config = Config::load()?;
    let (project_name, project, _repo_env) = config::resolve_project(&config, explicit_project)?;

    create_worktree_with_hooks(&project_name, &project, worktree_name, vcs_override)?;
    Ok(())
}

fn cmd_worktree_list(
    project_filter: Option<&str>,
    vcs_override: Option<vcs::VcsOverride>,
) -> Result<()> {
    let config = Config::load()?;

    let mut found_any = false;
    // Track seen repo paths (not names) to deduplicate when registered name ≠ effective_name()
    let mut seen_repo_paths = std::collections::HashSet::new();

    // 1. Iterate registered projects
    for (project_name, project) in &config.projects {
        if let Some(filter) = project_filter {
            if project_name != filter {
                continue;
            }
        }

        if let Ok(canonical) = project.path.canonicalize() {
            seen_repo_paths.insert(canonical);
        }
        let backend = vcs::detect_backend(&project.path, vcs_override)?;
        let worktrees = backend.list_worktrees(&project.path, &project.worktree_base())?;
        for wt in worktrees {
            found_any = true;
            let dir_name = wt
                .path
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            let branch = wt.branch.as_deref().unwrap_or(match wt.vcs_kind {
                vcs::VcsKind::Git => "(detached)",
                vcs::VcsKind::Jj => "(jj workspace)",
            });
            println!("{project_name}-{dir_name}\t{branch}\t{}", wt.path.display());
        }
    }

    // 2. Also include auto-detected project from cwd (if not already listed)
    let cwd = std::env::current_dir()?;
    let auto_detected = config::RepoConfig::discover(&cwd)?;
    if let Some((ref repo_config, ref repo_root)) = auto_detected {
        let name = repo_config.effective_name(repo_root);

        let matches_filter = match project_filter {
            Some(filter) => filter == name,
            None => true,
        };

        // Deduplicate by repo path — covers the case where registered name ≠ effective_name()
        if matches_filter && !seen_repo_paths.contains(repo_root) {
            let user_proj = config.projects.get(&name);
            let path = user_proj.map_or_else(|| repo_root.clone(), |p| p.path.clone());
            let project = config::merge_project(Some(repo_config), user_proj, path);
            let backend = vcs::detect_backend(&project.path, vcs_override)?;
            let worktrees = backend.list_worktrees(&project.path, &project.worktree_base())?;
            for wt in worktrees {
                found_any = true;
                let dir_name = wt
                    .path
                    .file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default();
                let branch = wt.branch.as_deref().unwrap_or(match wt.vcs_kind {
                    vcs::VcsKind::Git => "(detached)",
                    vcs::VcsKind::Jj => "(jj workspace)",
                });
                println!("{name}-{dir_name}\t{branch}\t{}", wt.path.display());
            }
        }
    }

    // 3. If filter was provided and nothing found, check validity
    if !found_any {
        if let Some(filter) = project_filter {
            if !config.projects.contains_key(filter) {
                // Reuse the cached auto-detection result instead of re-running discover()
                let auto_name = auto_detected.map(|(rc, root)| rc.effective_name(&root));
                if auto_name.as_deref() != Some(filter) {
                    return Err(Error::ProjectNotFound(filter.to_string()));
                }
            }
            println!("No worktrees found for project '{filter}'");
        } else {
            println!("No worktrees found");
        }
    }

    Ok(())
}

fn cleanup_and_remove_worktree(
    project_name: &str,
    project: &config::Project,
    wt: &vcs::WorktreeInfo,
    vcs_override: Option<vcs::VcsOverride>,
) -> Result<()> {
    // Extract worktree dir name before removal (directory may be gone after)
    let worktree_dir_name = wt
        .path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    if let Some(db_config) = &project.database {
        let db_name = db_config.db_name(project_name, &worktree_dir_name);
        println!("Dropping database '{db_name}'...");
        drop_database(&db_name)?;
        println!("Dropped database '{db_name}'");
    }

    let backend = vcs::detect_backend(&project.path, vcs_override)?;
    backend.remove_worktree(&project.path, &wt.path, &worktree_dir_name)?;
    println!("Removed worktree at {}", wt.path.display());

    if project.database.is_some() {
        let override_path = config::worktree_env_path(project_name, &worktree_dir_name)?;
        if override_path.exists() {
            std::fs::remove_file(&override_path)?;
        }
    }

    Ok(())
}

fn cmd_worktree_rm(name: &str, vcs_override: Option<vcs::VcsOverride>) -> Result<()> {
    let config = Config::load()?;

    let mut matches: Vec<(String, config::Project, vcs::WorktreeInfo)> = Vec::new();
    // Track seen repo paths (not names) to deduplicate when registered name ≠ effective_name()
    let mut seen_repo_paths = std::collections::HashSet::new();

    // 1. Search registered projects
    for (project_name, project) in &config.projects {
        if let Ok(canonical) = project.path.canonicalize() {
            seen_repo_paths.insert(canonical);
        }
        let backend = vcs::detect_backend(&project.path, vcs_override)?;
        let worktrees = backend.list_worktrees(&project.path, &project.worktree_base())?;
        for wt in worktrees {
            let dir_name = wt
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let full_name = format!("{project_name}-{dir_name}");
            if full_name == name {
                return cleanup_and_remove_worktree(project_name, project, &wt, vcs_override);
            }

            if dir_name == name {
                matches.push((project_name.clone(), project.clone(), wt));
            }
        }
    }

    // 2. Also search auto-detected project from cwd
    let cwd = std::env::current_dir()?;
    if let Some((repo_config, repo_root)) = config::RepoConfig::discover(&cwd)? {
        // Deduplicate by repo path — covers the case where registered name ≠ effective_name()
        if !seen_repo_paths.contains(&repo_root) {
            let proj_name = repo_config.effective_name(&repo_root);
            let user_proj = config.projects.get(&proj_name);
            let path = user_proj.map(|p| p.path.clone()).unwrap_or(repo_root);
            let project = config::merge_project(Some(&repo_config), user_proj, path);
            let backend = vcs::detect_backend(&project.path, vcs_override)?;
            let worktrees = backend.list_worktrees(&project.path, &project.worktree_base())?;
            for wt in worktrees {
                let dir_name = wt
                    .path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                let full_name = format!("{proj_name}-{dir_name}");
                if full_name == name {
                    return cleanup_and_remove_worktree(&proj_name, &project, &wt, vcs_override);
                }

                if dir_name == name {
                    matches.push((proj_name.clone(), project.clone(), wt));
                }
            }
        }
    }

    // 3. Handle matches
    match matches.len() {
        0 => Err(Error::WorktreeNotFound(name.to_string())),
        1 => {
            let (project_name, project, wt) = matches.remove(0);
            cleanup_and_remove_worktree(&project_name, &project, &wt, vcs_override)
        }
        _ => {
            let candidates: Vec<String> = matches
                .iter()
                .map(|(proj, _, wt)| {
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

fn mise_data_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("MISE_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return Ok(PathBuf::from(dir).join("mise"));
    }
    dirs::home_dir()
        .map(|h| h.join(".local/share/mise"))
        .ok_or(Error::NoDataDir)
}

fn cmd_init_mise() -> Result<()> {
    let plugin_dir = mise_data_dir()?.join("plugins/grove");
    let hooks_dir = plugin_dir.join("hooks");

    std::fs::create_dir_all(&hooks_dir)?;
    std::fs::write(plugin_dir.join("metadata.lua"), MISE_METADATA_LUA)?;
    std::fs::write(hooks_dir.join("mise_env.lua"), MISE_ENV_LUA)?;

    println!("Installed grove plugin to {}", plugin_dir.display());
    println!();
    println!("Add the following to ~/.config/mise/config.toml:");
    println!();
    println!("[env]");
    println!("_.grove = {{}}");
    println!();
    println!("If the file doesn't exist yet, create it first.");

    Ok(())
}
