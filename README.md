# Grove

Manage a grove of git/jj repositories with per-project environment variables and centralized worktree management.

Grove tracks multiple repositories, manages isolated worktrees for each, and provides layered per-project and per-worktree environment variables. It integrates with [mise](https://mise.jdx.dev/) to inject environment variables automatically.

## Installation

### Pre-built binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/coreyja-studio/grove/releases):

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/coreyja-studio/grove/releases/latest/download/grove-v0.1.0-aarch64-apple-darwin.tar.gz
tar xzf grove-v0.1.0-aarch64-apple-darwin.tar.gz
sudo mv grove /usr/local/bin/

# macOS (Intel)
curl -LO https://github.com/coreyja-studio/grove/releases/latest/download/grove-v0.1.0-x86_64-apple-darwin.tar.gz
tar xzf grove-v0.1.0-x86_64-apple-darwin.tar.gz
sudo mv grove /usr/local/bin/

# Linux (x86_64)
curl -LO https://github.com/coreyja-studio/grove/releases/latest/download/grove-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
tar xzf grove-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
sudo mv grove /usr/local/bin/
```

### From source

```bash
cargo install --git https://github.com/coreyja-studio/grove
```

## Quick Start

### 1. Register a project

```bash
grove add myproject /path/to/repo
```

### 2. Set environment variables

```bash
grove env set myproject DATABASE_URL postgres:///myproject_dev
grove env set myproject RUST_LOG debug
```

### 3. Set up mise integration

```bash
grove init-mise
```

Then add to `~/.config/mise/config.toml`:

```toml
[env]
_.grove = {}
```

Environment variables are now automatically injected when you `cd` into a grove-managed project.

### 4. Start working

```bash
grove start myproject my-feature
```

This creates a worktree, runs any configured hooks, and opens your `$EDITOR`.

## Features

### Project Registry

Track multiple repositories under short names:

```bash
grove add frontend ~/code/frontend
grove add backend ~/code/backend
grove list
```

### Layered Environment Variables

Environment variables resolve in three layers, highest priority first:

1. **Worktree-level** -- overrides for a specific worktree
2. **Project-level** -- defaults for the project (`grove env set`)
3. **Repo-level** -- defaults committed to the repo (`.grove/config.toml`)

```bash
# Project-wide default
grove env set myproject DATABASE_URL postgres:///myproject_dev

# Override for a specific worktree
grove env set myproject DATABASE_URL postgres:///myproject_feature --worktree my-feature

# View resolved variables
grove env list myproject
grove env export myproject --json
```

### Worktree Management

Create and manage git worktrees or jj workspaces:

```bash
grove worktree new myproject my-feature
grove worktree list myproject
grove worktree rm myproject my-feature
```

Grove auto-detects whether a project uses git or jj and creates the appropriate worktree type.

### Repo-Scoped Config

Commit a `.grove/config.toml` to your repo to share defaults with your team:

```toml
name = "myproject"

[database]
url_template = "postgres:///{{db_name}}"
setup_command = "cargo sqlx database setup"

[hooks]
post_create = ["yarn install", "cargo build"]

[env]
RUST_LOG = "debug"
NODE_ENV = "development"
```

When a teammate runs `grove start`, the hooks run automatically and env vars are set as defaults.

### mise Integration

Grove ships with a mise plugin that automatically exports your grove environment variables into the shell. After running `grove init-mise`, variables are injected whenever you enter a project directory -- no manual sourcing required.

## Commands

| Command | Description |
|---------|-------------|
| `grove add <name> <path>` | Register a git/jj repo |
| `grove list` | Show all registered projects |
| `grove remove <name>` | Unregister a project (doesn't delete files) |
| `grove env set <project> <key> <value>` | Set an environment variable |
| `grove env list <project>` | List environment variables |
| `grove env unset <project> <key>` | Remove an environment variable |
| `grove env export <project>` | Export variables (supports `--json`) |
| `grove worktree new <project> <name>` | Create a worktree |
| `grove worktree list <project>` | List worktrees |
| `grove worktree rm <project> <name>` | Remove a worktree |
| `grove start <project> <name>` | Create worktree + run hooks + open editor |
| `grove init-mise` | Install the grove mise plugin |

## Configuration

Grove stores its configuration at `~/.config/grove/config.toml`. This file is managed by grove commands -- you don't need to edit it directly.

Environment variable files live at `~/.config/grove/envs/`.

## License

MIT
