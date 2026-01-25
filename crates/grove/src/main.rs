use std::path::PathBuf;

use clap::Parser;

mod config;
mod error;

use config::{Config, EnvVars};
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
}

#[derive(clap::Subcommand)]
enum EnvCommands {
    /// Set a project-level environment variable
    Set {
        /// Project name
        project: String,
        /// KEY=value pair
        pair: String,
    },
    /// Show all environment variables for a project
    List {
        /// Project name
        project: String,
    },
    /// Output environment variables for the project containing a path
    Export {
        /// Directory path
        path: PathBuf,
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
            EnvCommands::Export { path } => cmd_env_export(path),
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
    // Validate project exists
    let config = Config::load()?;
    if !config.projects.contains_key(project) {
        return Err(Error::ProjectNotFound(project.to_string()));
    }

    let (key, value) = pair
        .split_once('=')
        .ok_or_else(|| Error::InvalidEnvFormat(pair.to_string()))?;

    let mut vars = EnvVars::load(project)?;
    vars.set(key.to_string(), value.to_string());
    vars.save(project)?;
    println!("Set {key} for project '{project}'");
    Ok(())
}

fn cmd_env_list(project: &str) -> Result<()> {
    // Validate project exists
    let config = Config::load()?;
    if !config.projects.contains_key(project) {
        return Err(Error::ProjectNotFound(project.to_string()));
    }

    let vars = EnvVars::load(project)?;
    if vars.vars.is_empty() {
        println!("No environment variables set for '{project}'");
        return Ok(());
    }
    for (key, value) in &vars.vars {
        println!("{key}={value}");
    }
    Ok(())
}

fn cmd_env_export(path: PathBuf) -> Result<()> {
    let config = Config::load()?;
    let project = config
        .find_project_for_path(&path)?
        .ok_or(Error::NoProjectForPath(path))?;

    let vars = EnvVars::load(&project)?;
    let output = vars.export();
    if !output.is_empty() {
        println!("{output}");
    }
    Ok(())
}
