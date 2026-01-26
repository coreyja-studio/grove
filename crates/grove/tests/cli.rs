use std::fs;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn grove_cmd(config_dir: &TempDir) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("grove"));
    cmd.env("GROVE_CONFIG_DIR", config_dir.path());
    cmd
}

/// Create a fake git repo (just a .git directory) for basic unit tests
fn create_fake_git_repo(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let repo_path = dir.path().join(name);
    fs::create_dir_all(&repo_path).unwrap();
    fs::create_dir(repo_path.join(".git")).unwrap();
    repo_path
}

/// Create a real git repo using `git init` for e2e tests
fn create_real_git_repo(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let repo_path = dir.path().join(name);
    fs::create_dir_all(&repo_path).unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to run git init");
    repo_path
}

/// Create a real git repo with an initial commit (required for worktrees)
fn create_real_git_repo_with_commit(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let repo_path = create_real_git_repo(dir, name);

    // Configure git user for this repo
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to set git email");
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to set git name");

    // Create a file and make an initial commit
    fs::write(repo_path.join("README.md"), "# Test Project\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&repo_path)
        .output()
        .expect("failed to git add");
    Command::new("git")
        .args(["commit", "-m", "Initial commit", "--no-gpg-sign"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to git commit");

    repo_path
}

#[test]
fn test_list_empty() {
    let config_dir = TempDir::new().unwrap();

    grove_cmd(&config_dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("No projects registered"));
}

#[test]
fn test_add_and_list() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    // Add project
    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added project 'myproject'"));

    // List should show it
    grove_cmd(&config_dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("myproject"));
}

#[test]
fn test_add_not_git_repo() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let not_repo = repos_dir.path().join("notgit");
    fs::create_dir_all(&not_repo).unwrap();

    grove_cmd(&config_dir)
        .args(["add", "test", not_repo.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a git repository"));
}

#[test]
fn test_add_path_not_found() {
    let config_dir = TempDir::new().unwrap();

    grove_cmd(&config_dir)
        .args(["add", "test", "/nonexistent/path/here"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path not found"));
}

#[test]
fn test_add_duplicate() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success();

    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_remove() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    // Add then remove
    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success();

    grove_cmd(&config_dir)
        .args(["remove", "myproject"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed project 'myproject'"));

    // List should be empty again
    grove_cmd(&config_dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("No projects registered"));
}

#[test]
fn test_remove_not_found() {
    let config_dir = TempDir::new().unwrap();

    grove_cmd(&config_dir)
        .args(["remove", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_env_set_and_list() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    // Add project first
    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success();

    // Set env var
    grove_cmd(&config_dir)
        .args(["env", "set", "myproject", "DATABASE_URL=postgres:///test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set DATABASE_URL"));

    // List env vars
    grove_cmd(&config_dir)
        .args(["env", "list", "myproject"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DATABASE_URL=postgres:///test"));
}

#[test]
fn test_env_list_empty() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success();

    grove_cmd(&config_dir)
        .args(["env", "list", "myproject"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No environment variables"));
}

#[test]
fn test_env_set_project_not_found() {
    let config_dir = TempDir::new().unwrap();

    grove_cmd(&config_dir)
        .args(["env", "set", "nonexistent", "FOO=bar"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_env_set_invalid_format() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success();

    grove_cmd(&config_dir)
        .args(["env", "set", "myproject", "NOEQUALSSIGN"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid KEY=value format"));
}

#[test]
fn test_env_export() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success();

    grove_cmd(&config_dir)
        .args(["env", "set", "myproject", "FOO=bar"])
        .assert()
        .success();

    grove_cmd(&config_dir)
        .args(["env", "set", "myproject", "BAZ=qux"])
        .assert()
        .success();

    // Export from exact path
    grove_cmd(&config_dir)
        .args(["env", "export", repo_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("export BAZ='qux'"))
        .stdout(predicate::str::contains("export FOO='bar'"));
}

#[test]
fn test_env_export_subdirectory() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();
    let repo_path = create_fake_git_repo(&repos_dir, "myproject");

    // Create a subdirectory
    let subdir = repo_path.join("src");
    fs::create_dir_all(&subdir).unwrap();

    grove_cmd(&config_dir)
        .args(["add", "myproject", repo_path.to_str().unwrap()])
        .assert()
        .success();

    grove_cmd(&config_dir)
        .args(["env", "set", "myproject", "KEY=value"])
        .assert()
        .success();

    // Export from subdirectory should still work
    grove_cmd(&config_dir)
        .args(["env", "export", subdir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("export KEY='value'"));
}

#[test]
fn test_env_export_no_project() {
    let config_dir = TempDir::new().unwrap();
    let repos_dir = TempDir::new().unwrap();

    grove_cmd(&config_dir)
        .args(["env", "export", repos_dir.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "does not belong to any registered project",
        ));
}

// =============================================================================
// E2E tests using real git repositories
// =============================================================================

mod e2e {
    use super::*;

    /// E2E test: Full workflow with a real git repository
    #[test]
    fn test_full_workflow_with_real_repo() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        // Create a real git repo
        let repo_path = create_real_git_repo(&repos_dir, "my-rust-project");

        // Add some files to make it realistic
        fs::write(repo_path.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();
        fs::create_dir_all(repo_path.join("src")).unwrap();
        fs::write(repo_path.join("src/main.rs"), "fn main() {}\n").unwrap();

        // Register the project
        grove_cmd(&config_dir)
            .args(["add", "rust-project", repo_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Added project 'rust-project'"));

        // Verify it's listed
        grove_cmd(&config_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("rust-project"))
            .stdout(predicate::str::contains("my-rust-project"));

        // Set multiple environment variables
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "rust-project",
                "DATABASE_URL=postgres:///mydb",
            ])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "rust-project", "RUST_LOG=debug"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "rust-project", "API_KEY=secret123"])
            .assert()
            .success();

        // List env vars
        grove_cmd(&config_dir)
            .args(["env", "list", "rust-project"])
            .assert()
            .success()
            .stdout(predicate::str::contains("DATABASE_URL=postgres:///mydb"))
            .stdout(predicate::str::contains("RUST_LOG=debug"))
            .stdout(predicate::str::contains("API_KEY=secret123"));

        // Export from project root
        grove_cmd(&config_dir)
            .args(["env", "export", repo_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "export DATABASE_URL='postgres:///mydb'",
            ))
            .stdout(predicate::str::contains("export RUST_LOG='debug'"))
            .stdout(predicate::str::contains("export API_KEY='secret123'"));

        // Export from src subdirectory
        grove_cmd(&config_dir)
            .args(["env", "export", repo_path.join("src").to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("export DATABASE_URL='"));

        // Update an existing env var
        grove_cmd(&config_dir)
            .args(["env", "set", "rust-project", "RUST_LOG=info"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "list", "rust-project"])
            .assert()
            .success()
            .stdout(predicate::str::contains("RUST_LOG=info"))
            .stdout(predicate::str::contains("RUST_LOG=debug").not());

        // Remove the project
        grove_cmd(&config_dir)
            .args(["remove", "rust-project"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("No projects registered"));
    }

    /// E2E test: Multiple real git repos
    #[test]
    fn test_multiple_real_repos() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        // Create multiple real repos
        let frontend = create_real_git_repo(&repos_dir, "frontend");
        let backend = create_real_git_repo(&repos_dir, "backend");
        let shared = create_real_git_repo(&repos_dir, "shared-libs");

        // Add all projects
        grove_cmd(&config_dir)
            .args(["add", "frontend", frontend.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["add", "backend", backend.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["add", "shared", shared.to_str().unwrap()])
            .assert()
            .success();

        // List should show all three
        let list_output = grove_cmd(&config_dir).arg("list").assert().success();

        list_output
            .stdout(predicate::str::contains("frontend"))
            .stdout(predicate::str::contains("backend"))
            .stdout(predicate::str::contains("shared"));

        // Set different env vars for each
        grove_cmd(&config_dir)
            .args(["env", "set", "frontend", "PORT=3000"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "backend", "PORT=8080"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "backend", "DATABASE_URL=postgres:///backend"])
            .assert()
            .success();

        // Export from each should give different results
        grove_cmd(&config_dir)
            .args(["env", "export", frontend.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("export PORT='3000'"));

        grove_cmd(&config_dir)
            .args(["env", "export", backend.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("export PORT='8080'"))
            .stdout(predicate::str::contains(
                "export DATABASE_URL='postgres:///backend'",
            ));

        // Shared has no env vars
        grove_cmd(&config_dir)
            .args(["env", "export", shared.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::is_empty());
    }

    /// E2E test: Deep subdirectory detection
    #[test]
    fn test_deep_subdirectory_detection() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo(&repos_dir, "monorepo");

        // Create deep directory structure
        let deep_path = repo.join("packages/core/src/utils/helpers");
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("index.ts"), "export const foo = 1;\n").unwrap();

        grove_cmd(&config_dir)
            .args(["add", "monorepo", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "monorepo", "NODE_ENV=development"])
            .assert()
            .success();

        // Export from deeply nested directory should work
        grove_cmd(&config_dir)
            .args(["env", "export", deep_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("export NODE_ENV='development'"));
    }

    /// E2E test: Values with special characters
    #[test]
    fn test_special_character_values() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo(&repos_dir, "project");

        grove_cmd(&config_dir)
            .args(["add", "project", repo.to_str().unwrap()])
            .assert()
            .success();

        // Value with spaces
        grove_cmd(&config_dir)
            .args(["env", "set", "project", "MESSAGE=hello world"])
            .assert()
            .success();

        // Value with special shell characters
        grove_cmd(&config_dir)
            .args(["env", "set", "project", "PATTERN=$HOME/*.txt"])
            .assert()
            .success();

        // Check export properly escapes
        grove_cmd(&config_dir)
            .args(["env", "export", repo.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("export MESSAGE='hello world'"))
            .stdout(predicate::str::contains("export PATTERN='$HOME/*.txt'"));
    }

    /// E2E test: Verify config persistence
    #[test]
    fn test_config_persistence() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo(&repos_dir, "persistent-project");

        // Add project and set env var
        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "SAVED=true"])
            .assert()
            .success();

        // Verify config files were created
        assert!(config_dir.path().join("config.toml").exists());
        assert!(config_dir.path().join("envs/myproject.toml").exists());

        // Read and verify config content
        let config_content = fs::read_to_string(config_dir.path().join("config.toml")).unwrap();
        assert!(config_content.contains("[projects.myproject]"));
        assert!(config_content.contains("path = "));

        let env_content =
            fs::read_to_string(config_dir.path().join("envs/myproject.toml")).unwrap();
        assert!(env_content.contains("SAVED = \"true\""));

        // A new grove command should still see the data
        grove_cmd(&config_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("myproject"));

        grove_cmd(&config_dir)
            .args(["env", "list", "myproject"])
            .assert()
            .success()
            .stdout(predicate::str::contains("SAVED=true"));
    }
}

// =============================================================================
// Worktree tests (require real git repositories with commits)
// =============================================================================

mod worktree {
    use super::*;

    #[test]
    fn test_worktree_new() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        // Add project
        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created worktree"));

        // Verify the worktree was created at the default location (.worktrees)
        let worktree_path = repo.join(".worktrees/feature");
        assert!(worktree_path.exists(), "Worktree directory should exist");
        assert!(
            worktree_path.join(".git").exists(),
            "Worktree should have a .git file"
        );
    }

    #[test]
    fn test_worktree_new_project_not_found() {
        let config_dir = TempDir::new().unwrap();

        grove_cmd(&config_dir)
            .args(["worktree", "new", "nonexistent", "feature"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn test_worktree_new_invalid_name() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Try to create worktree with invalid name
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "has spaces"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid worktree name"));

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "has/slash"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid worktree name"));
    }

    #[test]
    fn test_worktree_new_path_exists() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Create the worktree path manually first
        let worktree_path = repo.join(".worktrees/feature");
        fs::create_dir_all(&worktree_path).unwrap();

        // Try to create worktree - should fail
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("already exists"));
    }

    #[test]
    fn test_worktree_list_empty() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No worktrees found"));
    }

    #[test]
    fn test_worktree_list_shows_worktrees() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Create a worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .success();

        // List should show it
        grove_cmd(&config_dir)
            .args(["worktree", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("myproject-feature"))
            .stdout(predicate::str::contains("feature")); // branch name
    }

    #[test]
    fn test_worktree_list_filter_by_project() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo1 = create_real_git_repo_with_commit(&repos_dir, "project1");
        let repo2 = create_real_git_repo_with_commit(&repos_dir, "project2");

        grove_cmd(&config_dir)
            .args(["add", "proj1", repo1.to_str().unwrap()])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["add", "proj2", repo2.to_str().unwrap()])
            .assert()
            .success();

        // Create worktrees for both projects
        grove_cmd(&config_dir)
            .args(["worktree", "new", "proj1", "feature1"])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["worktree", "new", "proj2", "feature2"])
            .assert()
            .success();

        // List all - should show both
        grove_cmd(&config_dir)
            .args(["worktree", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("proj1-feature1"))
            .stdout(predicate::str::contains("proj2-feature2"));

        // List filtered to proj1 - should only show proj1's worktree
        grove_cmd(&config_dir)
            .args(["worktree", "list", "proj1"])
            .assert()
            .success()
            .stdout(predicate::str::contains("proj1-feature1"))
            .stdout(predicate::str::contains("proj2-feature2").not());

        // List filtered to proj2 - should only show proj2's worktree
        grove_cmd(&config_dir)
            .args(["worktree", "list", "proj2"])
            .assert()
            .success()
            .stdout(predicate::str::contains("proj2-feature2"))
            .stdout(predicate::str::contains("proj1-feature1").not());
    }

    #[test]
    fn test_worktree_list_project_not_found() {
        let config_dir = TempDir::new().unwrap();

        grove_cmd(&config_dir)
            .args(["worktree", "list", "nonexistent"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn test_worktree_rm() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .success();

        let worktree_path = repo.join(".worktrees/feature");
        assert!(worktree_path.exists());

        // Remove worktree by full name
        grove_cmd(&config_dir)
            .args(["worktree", "rm", "myproject-feature"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed worktree"));

        // Verify it's gone
        assert!(!worktree_path.exists());

        // List should be empty
        grove_cmd(&config_dir)
            .args(["worktree", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No worktrees found"));
    }

    #[test]
    fn test_worktree_rm_by_short_name() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .success();

        // Remove worktree by short name (unambiguous since only one project)
        grove_cmd(&config_dir)
            .args(["worktree", "rm", "feature"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed worktree"));
    }

    #[test]
    fn test_worktree_rm_not_found() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "rm", "nonexistent"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn test_worktree_rm_ambiguous() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo1 = create_real_git_repo_with_commit(&repos_dir, "project1");
        let repo2 = create_real_git_repo_with_commit(&repos_dir, "project2");

        grove_cmd(&config_dir)
            .args(["add", "proj1", repo1.to_str().unwrap()])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["add", "proj2", repo2.to_str().unwrap()])
            .assert()
            .success();

        // Create worktrees with same name in different projects
        grove_cmd(&config_dir)
            .args(["worktree", "new", "proj1", "feature"])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["worktree", "new", "proj2", "feature"])
            .assert()
            .success();

        // Try to remove by short name - should be ambiguous
        grove_cmd(&config_dir)
            .args(["worktree", "rm", "feature"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Ambiguous"))
            .stderr(predicate::str::contains("proj1-feature"))
            .stderr(predicate::str::contains("proj2-feature"));
    }

    #[test]
    fn test_env_export_from_worktree() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Set env var on project
        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "DATABASE_URL=postgres:///test"])
            .assert()
            .success();

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .success();

        let worktree_path = repo.join(".worktrees/feature");

        // Export from worktree path should return project's env vars
        grove_cmd(&config_dir)
            .args(["env", "export", worktree_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "export DATABASE_URL='postgres:///test'",
            ));
    }

    #[test]
    fn test_env_export_from_worktree_subdirectory() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "API_KEY=secret"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .success();

        // Create subdirectory in worktree
        let worktree_subdir = repo.join(".worktrees/feature/src");
        fs::create_dir_all(&worktree_subdir).unwrap();

        // Export from worktree subdirectory should still work
        grove_cmd(&config_dir)
            .args(["env", "export", worktree_subdir.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("export API_KEY='secret'"));
    }

    #[test]
    fn test_full_worktree_workflow() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        // Register project
        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Set env vars
        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "DATABASE_URL=postgres:///dev"])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "RUST_LOG=debug"])
            .assert()
            .success();

        // Create two worktrees
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature-a"])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature-b"])
            .assert()
            .success();

        // List all worktrees
        grove_cmd(&config_dir)
            .args(["worktree", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("myproject-feature-a"))
            .stdout(predicate::str::contains("myproject-feature-b"));

        // List filtered
        grove_cmd(&config_dir)
            .args(["worktree", "list", "myproject"])
            .assert()
            .success()
            .stdout(predicate::str::contains("feature-a"))
            .stdout(predicate::str::contains("feature-b"));

        // Export from worktree paths
        let wt_a = repo.join(".worktrees/feature-a");
        let wt_b = repo.join(".worktrees/feature-b");

        grove_cmd(&config_dir)
            .args(["env", "export", wt_a.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "export DATABASE_URL='postgres:///dev'",
            ))
            .stdout(predicate::str::contains("export RUST_LOG='debug'"));

        grove_cmd(&config_dir)
            .args(["env", "export", wt_b.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("export DATABASE_URL="));

        // Remove worktrees
        grove_cmd(&config_dir)
            .args(["worktree", "rm", "myproject-feature-a"])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["worktree", "rm", "feature-b"])
            .assert()
            .success();

        // List should be empty
        grove_cmd(&config_dir)
            .args(["worktree", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No worktrees found"));
    }
}
