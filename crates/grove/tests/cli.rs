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

// =============================================================================
// Worktree environment override tests
// =============================================================================

mod worktree_env {
    use super::*;

    #[test]
    fn test_env_set_worktree_override() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "discord"])
            .assert()
            .success();

        // Set worktree override
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject/discord",
                "DATABASE_URL=postgres:///test",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Set DATABASE_URL"))
            .stdout(predicate::str::contains("myproject/discord"));

        // Verify the override file was created
        let override_path = config_dir.path().join("envs/myproject/discord.toml");
        assert!(
            override_path.exists(),
            "Worktree override file should exist"
        );

        let content = fs::read_to_string(&override_path).unwrap();
        assert!(content.contains("DATABASE_URL"));
    }

    #[test]
    fn test_env_list_worktree_merged() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Set project-level var
        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "RUST_LOG=debug"])
            .assert()
            .success();

        // Set project-level var that will be overridden
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject",
                "DATABASE_URL=postgres:///default",
            ])
            .assert()
            .success();

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "discord"])
            .assert()
            .success();

        // Set worktree override for DATABASE_URL
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject/discord",
                "DATABASE_URL=postgres:///test",
            ])
            .assert()
            .success();

        // List worktree env - should show merged with indicators
        let output = grove_cmd(&config_dir)
            .args(["env", "list", "myproject/discord"])
            .assert()
            .success();

        output
            .stdout(predicate::str::contains("DATABASE_URL"))
            .stdout(predicate::str::contains("postgres:///test"))
            .stdout(predicate::str::contains("(override)"))
            .stdout(predicate::str::contains("RUST_LOG"))
            .stdout(predicate::str::contains("(from project)"));
    }

    #[test]
    fn test_env_list_project_backward_compatible() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo_path = create_fake_git_repo(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo_path.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "DATABASE_URL=postgres:///test"])
            .assert()
            .success();

        // Project-level list should still use KEY=value format (no annotations)
        grove_cmd(&config_dir)
            .args(["env", "list", "myproject"])
            .assert()
            .success()
            .stdout(predicate::str::contains("DATABASE_URL=postgres:///test"));
    }

    #[test]
    fn test_env_export_worktree_override() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Set project-level vars
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject",
                "DATABASE_URL=postgres:///default",
            ])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "RUST_LOG=debug"])
            .assert()
            .success();

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "discord"])
            .assert()
            .success();

        // Set worktree override
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject/discord",
                "DATABASE_URL=postgres:///test",
            ])
            .assert()
            .success();

        let worktree_path = repo.join(".worktrees/discord");

        // Export from worktree path should return overridden value
        grove_cmd(&config_dir)
            .args(["env", "export", worktree_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "export DATABASE_URL='postgres:///test'",
            ))
            .stdout(predicate::str::contains("export RUST_LOG='debug'"));

        // Export from main repo path should still return project default
        grove_cmd(&config_dir)
            .args(["env", "export", repo.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "export DATABASE_URL='postgres:///default'",
            ));
    }

    #[test]
    fn test_env_unset_worktree() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "discord"])
            .assert()
            .success();

        // Set two worktree overrides
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject/discord",
                "DATABASE_URL=postgres:///test",
            ])
            .assert()
            .success();
        grove_cmd(&config_dir)
            .args(["env", "set", "myproject/discord", "EXTRA=value"])
            .assert()
            .success();

        // Unset one key
        grove_cmd(&config_dir)
            .args(["env", "unset", "myproject/discord", "DATABASE_URL"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Unset DATABASE_URL"));

        // Override file should still exist (still has EXTRA)
        let override_path = config_dir.path().join("envs/myproject/discord.toml");
        assert!(override_path.exists());
    }

    #[test]
    fn test_env_unset_worktree_last_key_removes_file() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "discord"])
            .assert()
            .success();

        // Set one worktree override
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject/discord",
                "DATABASE_URL=postgres:///test",
            ])
            .assert()
            .success();

        let override_path = config_dir.path().join("envs/myproject/discord.toml");
        assert!(override_path.exists());

        // Unset the only key
        grove_cmd(&config_dir)
            .args(["env", "unset", "myproject/discord", "DATABASE_URL"])
            .assert()
            .success();

        // Override file should be deleted
        assert!(
            !override_path.exists(),
            "Override file should be deleted when last key removed"
        );
    }

    #[test]
    fn test_env_unset_project_last_key_removes_file() {
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

        let env_path = config_dir.path().join("envs/myproject.toml");
        assert!(env_path.exists());

        // Unset the only key
        grove_cmd(&config_dir)
            .args(["env", "unset", "myproject", "FOO"])
            .assert()
            .success();

        // Env file should be deleted
        assert!(
            !env_path.exists(),
            "Env file should be deleted when last key removed"
        );
    }

    #[test]
    fn test_env_list_worktree_empty() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "discord"])
            .assert()
            .success();

        // List worktree env with no vars set
        grove_cmd(&config_dir)
            .args(["env", "list", "myproject/discord"])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "No environment variables set for 'myproject/discord'",
            ));
    }

    #[test]
    fn test_env_set_worktree_not_found() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo_path = create_fake_git_repo(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo_path.to_str().unwrap()])
            .assert()
            .success();

        // Try to set env on nonexistent worktree
        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject/nonexistent",
                "DATABASE_URL=postgres:///test",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }
}

// =============================================================================
// JSON export tests
// =============================================================================

mod json_export {
    use super::*;

    #[test]
    fn test_env_export_json_registered_project() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo_path = create_fake_git_repo(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo_path.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "DATABASE_URL=postgres:///test"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "API_KEY=secret"])
            .assert()
            .success();

        // Export as JSON
        grove_cmd(&config_dir)
            .args(["env", "export", "--json", repo_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                r#"{"API_KEY":"secret","DATABASE_URL":"postgres:///test"}"#,
            ));
    }

    #[test]
    fn test_env_export_json_nonexistent_path() {
        let config_dir = TempDir::new().unwrap();

        grove_cmd(&config_dir)
            .args(["env", "export", "--json", "/some/nonexistent/path"])
            .assert()
            .success()
            .stdout(predicate::str::contains("{}"));
    }

    #[test]
    fn test_env_export_json_unregistered_path() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        // An existing directory that isn't registered as a grove project
        grove_cmd(&config_dir)
            .args([
                "env",
                "export",
                "--json",
                repos_dir.path().to_str().unwrap(),
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("{}"));
    }

    #[test]
    fn test_env_export_json_worktree_overrides() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Set project-level var
        grove_cmd(&config_dir)
            .args(["env", "set", "myproject", "DATABASE_URL=postgres:///prod"])
            .assert()
            .success();

        // Create worktree and set override
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "feature"])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args([
                "env",
                "set",
                "myproject/feature",
                "DATABASE_URL=postgres:///dev",
            ])
            .assert()
            .success();

        let worktree_path = repo.join(".worktrees/feature");

        // JSON export from worktree should show overridden value
        grove_cmd(&config_dir)
            .args(["env", "export", "--json", worktree_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                r#""DATABASE_URL":"postgres:///dev""#,
            ));
    }

    #[test]
    fn test_env_export_json_no_env_vars() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo_path = create_fake_git_repo(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo_path.to_str().unwrap()])
            .assert()
            .success();

        // Don't set any env vars — JSON should return {}
        grove_cmd(&config_dir)
            .args(["env", "export", "--json", repo_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("{}"));
    }

    #[test]
    fn test_env_export_without_json_unchanged() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();

        // Non-JSON export of unregistered path should still error
        grove_cmd(&config_dir)
            .args(["env", "export", repos_dir.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "does not belong to any registered project",
            ));
    }
}

// =============================================================================
// init-mise tests
// =============================================================================

// =============================================================================
// Database provisioning tests
// =============================================================================

mod database {
    use super::*;

    fn postgres_available() -> bool {
        // Check that psql can actually connect, not just that the binary exists.
        // On CI runners the binaries are installed but the server isn't running.
        Command::new("psql")
            .args(["-c", "SELECT 1"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Write a config file with a database section for a project.
    fn write_config_with_database(
        config_dir: &TempDir,
        project_name: &str,
        project_path: &std::path::Path,
        url_template: &str,
        setup_command: Option<&str>,
    ) {
        let setup_line = setup_command
            .map(|cmd| format!("setup_command = \"{cmd}\""))
            .unwrap_or_default();
        let config_content = format!(
            r#"[projects.{project_name}]
path = "{}"

[projects.{project_name}.database]
url_template = "{url_template}"
{setup_line}
"#,
            project_path.display()
        );
        let config_path = config_dir.path().join("config.toml");
        fs::write(&config_path, config_content).unwrap();
    }

    #[test]
    fn test_worktree_creation_with_database() {
        if !postgres_available() {
            return;
        }

        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");
        let canonical = repo.canonicalize().unwrap();

        let db_name = "myproject_testfeat";

        // Cleanup from any previous failed runs
        let _ = Command::new("dropdb")
            .args(["--if-exists", db_name])
            .output();

        write_config_with_database(
            &config_dir,
            "myproject",
            &canonical,
            "postgres:///{{db_name}}",
            None,
        );

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "testfeat"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Creating database"))
            .stdout(predicate::str::contains("Created database"))
            .stdout(predicate::str::contains("Set DATABASE_URL"));

        // Verify database exists
        let psql_output = Command::new("psql")
            .args(["-d", db_name, "-c", "SELECT 1"])
            .output()
            .expect("failed to run psql");
        assert!(
            psql_output.status.success(),
            "Database should exist and be queryable"
        );

        // Verify env override file exists
        let override_path = config_dir.path().join("envs/myproject/testfeat.toml");
        assert!(override_path.exists(), "Env override file should exist");

        // Verify grove env export includes the database URL
        let worktree_path = canonical.join(".worktrees/testfeat");
        grove_cmd(&config_dir)
            .args(["env", "export", worktree_path.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "export DATABASE_URL='postgres:///myproject_testfeat'",
            ));

        // Cleanup
        let _ = Command::new("dropdb")
            .args(["--if-exists", db_name])
            .output();
    }

    #[test]
    fn test_worktree_removal_with_database_cleanup() {
        if !postgres_available() {
            return;
        }

        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");
        let canonical = repo.canonicalize().unwrap();

        let db_name = "myproject_rmfeat";

        // Cleanup from any previous failed runs
        let _ = Command::new("dropdb")
            .args(["--if-exists", db_name])
            .output();

        write_config_with_database(
            &config_dir,
            "myproject",
            &canonical,
            "postgres:///{{db_name}}",
            None,
        );

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "rmfeat"])
            .assert()
            .success();

        // Verify database was created
        let psql_output = Command::new("psql")
            .args(["-d", db_name, "-c", "SELECT 1"])
            .output()
            .expect("failed to run psql");
        assert!(psql_output.status.success(), "Database should exist");

        // Remove worktree
        grove_cmd(&config_dir)
            .args(["worktree", "rm", "myproject-rmfeat"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Dropping database"))
            .stdout(predicate::str::contains("Dropped database"))
            .stdout(predicate::str::contains("Removed worktree"));

        // Verify database is gone
        let psql_output = Command::new("psql")
            .args(["-d", db_name, "-c", "SELECT 1"])
            .output()
            .expect("failed to run psql");
        assert!(
            !psql_output.status.success(),
            "Database should not exist after removal"
        );

        // Verify env override file is gone
        let override_path = config_dir.path().join("envs/myproject/rmfeat.toml");
        assert!(
            !override_path.exists(),
            "Env override file should be removed"
        );

        // Cleanup (just in case)
        let _ = Command::new("dropdb")
            .args(["--if-exists", db_name])
            .output();
    }

    #[test]
    fn test_worktree_creation_without_database_config() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        // Add project normally (no database config)
        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Create worktree
        let output = grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "nodb"])
            .assert()
            .success();

        // Should NOT mention database
        output
            .stdout(predicate::str::contains("Creating database").not())
            .stdout(predicate::str::contains("DATABASE_URL").not());

        // No env override file should be created
        let override_path = config_dir.path().join("envs/myproject/nodb.toml");
        assert!(
            !override_path.exists(),
            "No env override file should be created without database config"
        );
    }

    #[test]
    fn test_setup_command_runs() {
        if !postgres_available() {
            return;
        }

        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");
        let canonical = repo.canonicalize().unwrap();

        let db_name = "myproject_setupfeat";

        // Cleanup from any previous failed runs
        let _ = Command::new("dropdb")
            .args(["--if-exists", db_name])
            .output();

        write_config_with_database(
            &config_dir,
            "myproject",
            &canonical,
            "postgres:///{{db_name}}",
            Some("touch .db-setup-done"),
        );

        // Create worktree
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "setupfeat"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Running setup command"))
            .stdout(predicate::str::contains("Setup command completed"));

        // Verify the setup command ran
        let marker_path = canonical.join(".worktrees/setupfeat/.db-setup-done");
        assert!(
            marker_path.exists(),
            "Setup command should have created .db-setup-done in worktree"
        );

        // Cleanup
        let _ = Command::new("dropdb")
            .args(["--if-exists", db_name])
            .output();
    }
}

mod hooks {
    use super::*;

    fn write_config_with_hooks(
        config_dir: &TempDir,
        project_name: &str,
        project_path: &std::path::Path,
        hooks: &[&str],
    ) {
        let hooks_toml: Vec<String> = hooks.iter().map(|h| format!("\"{h}\"")).collect();
        let hooks_array = hooks_toml.join(", ");
        let config_content = format!(
            r#"[projects.{project_name}]
path = "{}"

[projects.{project_name}.hooks]
post_create = [{hooks_array}]
"#,
            project_path.display()
        );
        let config_path = config_dir.path().join("config.toml");
        fs::write(&config_path, config_content).unwrap();
    }

    #[test]
    fn test_worktree_new_runs_post_create_hooks() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");
        let canonical = repo.canonicalize().unwrap();

        write_config_with_hooks(&config_dir, "myproject", &canonical, &["touch .hook-ran"]);

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "hookfeat"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Running hook"))
            .stdout(predicate::str::contains("Hook completed"));

        let marker_path = canonical.join(".worktrees/hookfeat/.hook-ran");
        assert!(
            marker_path.exists(),
            "Hook should have created .hook-ran in worktree"
        );
    }

    #[test]
    fn test_worktree_new_hook_failure_stops_execution() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");
        let canonical = repo.canonicalize().unwrap();

        write_config_with_hooks(
            &config_dir,
            "myproject",
            &canonical,
            &["false", "touch .should-not-exist"],
        );

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "failhook"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Post-create hook failed"));

        let marker_path = canonical.join(".worktrees/failhook/.should-not-exist");
        assert!(
            !marker_path.exists(),
            "Second hook should not have run after first hook failed"
        );
    }

    #[test]
    fn test_worktree_new_no_hooks_backward_compat() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "nohooks"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Running hook").not())
            .stdout(predicate::str::contains("Hook completed").not());
    }

    #[test]
    fn test_worktree_new_mise_trust_runs() {
        let config_dir = TempDir::new().unwrap();
        let repos_dir = TempDir::new().unwrap();
        let repo = create_real_git_repo_with_commit(&repos_dir, "myproject");

        grove_cmd(&config_dir)
            .args(["add", "myproject", repo.to_str().unwrap()])
            .assert()
            .success();

        // Should succeed regardless of whether mise is installed
        grove_cmd(&config_dir)
            .args(["worktree", "new", "myproject", "misetrust"])
            .assert()
            .success();
    }
}

mod init_mise {
    use super::*;

    #[test]
    fn test_init_mise_creates_plugin_files() {
        let config_dir = TempDir::new().unwrap();
        let data_dir = TempDir::new().unwrap();

        grove_cmd(&config_dir)
            .arg("init-mise")
            .env("MISE_DATA_DIR", data_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("Installed grove plugin"))
            .stdout(predicate::str::contains("~/.config/mise/config.toml"));

        // Check metadata.lua
        let metadata_path = data_dir.path().join("plugins/grove/metadata.lua");
        assert!(metadata_path.exists(), "metadata.lua should exist");
        let metadata = fs::read_to_string(&metadata_path).unwrap();
        assert!(metadata.contains(r#"PLUGIN.name = "grove""#));

        // Check hooks/mise_env.lua
        let hook_path = data_dir.path().join("plugins/grove/hooks/mise_env.lua");
        assert!(hook_path.exists(), "mise_env.lua should exist");
        let hook = fs::read_to_string(&hook_path).unwrap();
        assert!(hook.contains("MiseEnv"));
        assert!(hook.contains(r#"require("cmd")"#));
        assert!(hook.contains(r#"require("json")"#));
    }

    #[test]
    fn test_init_mise_idempotent() {
        let config_dir = TempDir::new().unwrap();
        let data_dir = TempDir::new().unwrap();

        // Run twice
        grove_cmd(&config_dir)
            .arg("init-mise")
            .env("MISE_DATA_DIR", data_dir.path())
            .assert()
            .success();

        grove_cmd(&config_dir)
            .arg("init-mise")
            .env("MISE_DATA_DIR", data_dir.path())
            .assert()
            .success();

        // Files should exist and be valid after both runs
        let metadata_path = data_dir.path().join("plugins/grove/metadata.lua");
        assert!(metadata_path.exists());
        let metadata = fs::read_to_string(&metadata_path).unwrap();
        assert!(metadata.contains(r#"PLUGIN.name = "grove""#));
    }
}
