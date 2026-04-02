# Jackin Workspaces And Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class saved workspaces, explicit workspace-aware `load` modes, and a fast interactive `launch` flow that starts from the current directory or a saved workspace.

**Architecture:** Introduce a dedicated `workspace` domain module for mount spec parsing, validation, and runtime workspace resolution; keep persisted workspace definitions in `AppConfig`; and add a separate `launch` Ratatui module that resolves a workspace/agent pair before handing off to the existing Docker runtime. Refactor `runtime::load_agent` so agent repo checkouts remain build inputs only, while resolved workspace mounts become the actual runtime filesystem layout; in the launcher preview, show unscoped global mounts separately, then merge selector-scoped mounts only during final `load` resolution.

**Tech Stack:** Rust, clap, serde/toml, ratatui, crossterm, owo-colors, cargo-nextest

---

## File Structure

| File | Responsibility | Action |
|------|----------------|--------|
| `Cargo.toml` | Add `ratatui` and `crossterm` dependencies for the launcher TUI | Modify |
| `src/workspace.rs` | Workspace domain types, mount spec parsing, validation, current-directory/saved/custom workspace resolution | Create |
| `src/launch.rs` | Ratatui launcher state machine, filtering, rendering, and user input loop | Create |
| `src/config.rs` | Persist saved workspaces in config; workspace CRUD helpers; validation wiring | Modify |
| `src/cli.rs` | Add `launch` and `workspace` commands; extend `load` grammar with `-w/--workspace`, path, mount, and workdir modes | Modify |
| `src/lib.rs` | Route `launch`, `workspace`, and new `load` modes into workspace resolution and runtime | Modify |
| `src/runtime.rs` | Accept `ResolvedWorkspace`, mount workspace paths instead of cached repo checkout, set container `--workdir`, pass host UID/GID build args | Modify |
| `src/derived_image.rs` | Reconcile the `claude` user UID/GID with the invoking host user during image build | Modify |
| `README.md` | Document `launch`, `workspace`, current-directory behavior, and explicit `load` modes | Modify |

---

### Task 1: Add Workspace Domain Types And Persisted Config Support

**Files:**
- Create: `src/workspace.rs`
- Modify: `src/config.rs`
- Modify: `src/lib.rs`
- Test: `src/config.rs`
- Test: `src/workspace.rs`

- [ ] **Step 1: Write failing config tests for saved workspace deserialization and validation**

Add these tests near the existing config tests in `src/config.rs`:

```rust
    #[test]
    fn deserializes_saved_workspaces() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/Users/donbeave/Projects/chainargos/big-monorepo"
default_agent = "agent-smith"
allowed_agents = ["agent-smith", "chainargos/the-architect"]

[[workspaces.big-monorepo.mounts]]
src = "/Users/donbeave/Projects/chainargos/big-monorepo"
dst = "/Users/donbeave/Projects/chainargos/big-monorepo"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/cache"
dst = "/workspace/cache"
readonly = true
"#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let workspace = config.workspaces.get("big-monorepo").unwrap();

        assert_eq!(workspace.workdir, "/Users/donbeave/Projects/chainargos/big-monorepo");
        assert_eq!(workspace.mounts.len(), 2);
        assert_eq!(workspace.default_agent.as_deref(), Some("agent-smith"));
        assert_eq!(workspace.allowed_agents.len(), 2);
        assert!(workspace.mounts[1].readonly);
    }

    #[test]
    fn rejects_workspace_with_workdir_outside_mounts() {
        let workspace = crate::workspace::WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: "/tmp/project".to_string(),
                dst: "/workspace/src".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
        };

        let error = crate::workspace::validate_workspace_config("big-monorepo", &workspace)
            .unwrap_err();

        assert!(error.to_string().contains("must be equal to or inside one of the workspace mount destinations"));
    }
```

- [ ] **Step 2: Run the targeted tests and confirm they fail**

Run: `cargo nextest run -E 'test(deserializes_saved_workspaces | rejects_workspace_with_workdir_outside_mounts)'`

Expected: FAIL because `AppConfig` has no `workspaces` field and `crate::workspace` does not exist yet.

- [ ] **Step 3: Create `src/workspace.rs` with shared mount/workspace types and validation helpers**

Create `src/workspace.rs` with this initial implementation:

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    pub workdir: String,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub allowed_agents: Vec<String>,
    #[serde(default)]
    pub default_agent: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub allowed_agents_to_add: Vec<String>,
    pub allowed_agents_to_remove: Vec<String>,
    pub default_agent: Option<Option<String>>,
}

pub fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/")) && let Ok(home) = std::env::var("HOME") {
        return path.replacen('~', &home, 1);
    }
    path.to_string()
}

pub fn parse_mount_spec(spec: &str) -> anyhow::Result<MountConfig> {
    let (raw, readonly) = match spec.strip_suffix(":ro") {
        Some(value) => (value, true),
        None => (spec, false),
    };
    let (src, dst) = raw
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid mount spec {spec:?}; expected src:dst[:ro]"))?;

    Ok(MountConfig {
        src: expand_tilde(src),
        dst: dst.to_string(),
        readonly,
    })
}

pub fn validate_mounts(mounts: &[MountConfig]) -> anyhow::Result<()> {
    let mut seen_dst = std::collections::HashSet::new();

    for mount in mounts {
        if !Path::new(&mount.src).is_absolute() {
            anyhow::bail!("mount source must be absolute: {}", mount.src);
        }
        if !Path::new(&mount.src).exists() {
            anyhow::bail!("mount source does not exist: {}", mount.src);
        }
        if !mount.dst.starts_with('/') {
            anyhow::bail!("mount destination must be an absolute path: {}", mount.dst);
        }
        if !seen_dst.insert(mount.dst.clone()) {
            anyhow::bail!("duplicate mount destination: {}", mount.dst);
        }
    }

    Ok(())
}

pub fn validate_workspace_config(name: &str, workspace: &WorkspaceConfig) -> anyhow::Result<()> {
    if workspace.workdir.is_empty() {
        anyhow::bail!("workspace {name:?} must define workdir");
    }
    if !workspace.workdir.starts_with('/') {
        anyhow::bail!("workspace {name:?} workdir must be an absolute container path");
    }
    if workspace.mounts.is_empty() {
        anyhow::bail!("workspace {name:?} must define at least one mount");
    }

    validate_mounts(&workspace.mounts)?;

    let within_mount = workspace.mounts.iter().any(|mount| {
        workspace.workdir == mount.dst
            || workspace.workdir.starts_with(&format!("{}/", mount.dst.trim_end_matches('/')))
    });
    anyhow::ensure!(
        within_mount,
        "workspace {name:?} workdir must be equal to or inside one of the workspace mount destinations"
    );

    if let Some(default_agent) = &workspace.default_agent
        && !workspace.allowed_agents.is_empty()
        && !workspace.allowed_agents.iter().any(|agent| agent == default_agent)
    {
        anyhow::bail!(
            "workspace {name:?} default_agent must be a member of allowed_agents when allowed_agents is set"
        );
    }

    Ok(())
}

pub fn current_dir_workspace(cwd: &Path) -> anyhow::Result<WorkspaceConfig> {
    let cwd = cwd.canonicalize()?;
    let path = cwd.display().to_string();
    Ok(WorkspaceConfig {
        workdir: path.clone(),
        mounts: vec![MountConfig {
            src: path.clone(),
            dst: path,
            readonly: false,
        }],
        allowed_agents: vec![],
        default_agent: None,
    })
}
```

Also register the module in `src/lib.rs`:

```rust
pub mod workspace;
```

- [ ] **Step 4: Extend `AppConfig` with persisted workspaces and CRUD helpers**

In `src/config.rs`, import the shared types and update `AppConfig`:

```rust
use crate::workspace::{validate_workspace_config, MountConfig, WorkspaceConfig, WorkspaceEdit};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSource>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}
```

Then add these methods to `impl AppConfig`:

```rust
    pub fn add_workspace(&mut self, name: &str, workspace: WorkspaceConfig) -> anyhow::Result<()> {
        validate_workspace_config(name, &workspace)?;
        self.workspaces.insert(name.to_string(), workspace);
        Ok(())
    }

    pub fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()> {
        let workspace = self
            .workspaces
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?;

        if let Some(workdir) = edit.workdir {
            workspace.workdir = workdir;
        }

        for dst in edit.remove_destinations {
            workspace.mounts.retain(|mount| mount.dst != dst);
        }

        for mount in edit.upsert_mounts {
            if let Some(existing) = workspace.mounts.iter_mut().find(|existing| existing.dst == mount.dst) {
                *existing = mount;
            } else {
                workspace.mounts.push(mount);
            }
        }

        for selector in edit.allowed_agents_to_add {
            if !workspace.allowed_agents.iter().any(|existing| existing == &selector) {
                workspace.allowed_agents.push(selector);
            }
        }

        for selector in edit.allowed_agents_to_remove {
            workspace.allowed_agents.retain(|existing| existing != &selector);
        }

        if let Some(default_agent) = edit.default_agent {
            workspace.default_agent = default_agent;
        }

        validate_workspace_config(name, workspace)?;
        Ok(())
    }

    pub fn remove_workspace(&mut self, name: &str) -> bool {
        self.workspaces.remove(name).is_some()
    }

    pub fn list_workspaces(&self) -> Vec<(&str, &WorkspaceConfig)> {
        self.workspaces
            .iter()
            .map(|(name, workspace)| (name.as_str(), workspace))
            .collect()
    }

    pub fn global_mounts(&self) -> Vec<MountConfig> {
        self.docker
            .mounts
            .0
            .iter()
            .filter_map(|(_, entry)| match entry {
                MountEntry::Mount(mount) => Some(mount.clone()),
                MountEntry::Scoped(_) => None,
            })
            .collect()
    }
```

Remove the old local `expand_tilde` and update any callers to use `crate::workspace::expand_tilde`.

- [ ] **Step 5: Add a focused workspace domain test module**

At the bottom of `src/workspace.rs`, add these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_mount_spec_with_optional_readonly_suffix() {
        let mount = parse_mount_spec("/tmp/cache:/workspace/cache:ro").unwrap();
        assert_eq!(mount.src, "/tmp/cache");
        assert_eq!(mount.dst, "/workspace/cache");
        assert!(mount.readonly);
    }

    #[test]
    fn current_dir_workspace_uses_same_host_and_container_path() {
        let dir = tempdir().unwrap();
        let workspace = current_dir_workspace(dir.path()).unwrap();

        assert_eq!(workspace.workdir, dir.path().canonicalize().unwrap().display().to_string());
        assert_eq!(workspace.mounts.len(), 1);
        assert_eq!(workspace.mounts[0].src, workspace.mounts[0].dst);
    }
}
```

- [ ] **Step 6: Run the targeted tests and confirm they pass**

Run: `cargo nextest run -E 'test(deserializes_saved_workspaces | rejects_workspace_with_workdir_outside_mounts | parses_mount_spec_with_optional_readonly_suffix | current_dir_workspace_uses_same_host_and_container_path)'`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/workspace.rs src/config.rs src/lib.rs
git commit -m "feat: add persisted workspace config model"
```

---

### Task 2: Add `workspace` CLI Commands And Wire CRUD Routing

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/lib.rs`
- Modify: `src/config.rs`
- Test: `src/cli.rs`

- [ ] **Step 1: Write failing CLI parsing tests for workspace commands**

Add these tests to `src/cli.rs`:

```rust
    #[test]
    fn parses_workspace_add_command() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "add",
            "big-monorepo",
            "--workdir",
            "/workspace/project",
            "--mount",
            "/tmp/project:/workspace/project",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
            "--allowed-agent",
            "agent-smith",
            "--default-agent",
            "agent-smith",
        ]).unwrap();

        assert!(matches!(
            cli.command,
            Command::Workspace { command: WorkspaceCommand::Add { .. } }
        ));
    }

    #[test]
    fn parses_workspace_edit_command() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "big-monorepo",
            "--mount",
            "/tmp/new-cache:/workspace/cache:ro",
            "--remove-destination",
            "/workspace/shared",
        ]).unwrap();

        assert!(matches!(
            cli.command,
            Command::Workspace { command: WorkspaceCommand::Edit { .. } }
        ));
    }
```

- [ ] **Step 2: Run the CLI tests and confirm they fail**

Run: `cargo nextest run -E 'test(parses_workspace_add_command | parses_workspace_edit_command)'`

Expected: FAIL because `Command::Workspace` and `WorkspaceCommand` do not exist yet.

- [ ] **Step 3: Add `workspace` subcommands to `src/cli.rs`**

Extend `Command` with a new variant and add the command enums below `ConfigCommand`:

```rust
    /// Manage saved workspaces
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
```

```rust
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceCommand {
    Add {
        name: String,
        #[arg(long)]
        workdir: String,
        #[arg(long = "mount", required = true)]
        mounts: Vec<String>,
        #[arg(long = "allowed-agent")]
        allowed_agents: Vec<String>,
        #[arg(long = "default-agent")]
        default_agent: Option<String>,
    },
    List,
    Show {
        name: String,
    },
    Edit {
        name: String,
        #[arg(long)]
        workdir: Option<String>,
        #[arg(long = "mount")]
        mounts: Vec<String>,
        #[arg(long = "remove-destination")]
        remove_destinations: Vec<String>,
        #[arg(long = "allowed-agent")]
        allowed_agents: Vec<String>,
        #[arg(long = "remove-allowed-agent")]
        remove_allowed_agents: Vec<String>,
        #[arg(long = "default-agent")]
        default_agent: Option<String>,
        #[arg(long = "clear-default-agent", default_value_t = false)]
        clear_default_agent: bool,
    },
    Remove {
        name: String,
    },
}
```

- [ ] **Step 4: Route workspace commands in `src/lib.rs`**

Import the new types:

```rust
use cli::{Cli, Command, WorkspaceCommand};
use workspace::{parse_mount_spec, WorkspaceConfig, WorkspaceEdit};
```

Add a new `match` arm before `Command::Purge`:

```rust
        Command::Workspace { command } => match command {
            WorkspaceCommand::Add { name, workdir, mounts, allowed_agents, default_agent } => {
                let mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec(value))
                    .collect::<Result<Vec<_>>>()?;
                config.add_workspace(
                    &name,
                    WorkspaceConfig {
                        workdir,
                        mounts,
                        allowed_agents,
                        default_agent,
                    },
                )?;
                config.save(&paths)?;
                Ok(())
            }
            WorkspaceCommand::List => {
                for (name, workspace) in config.list_workspaces() {
                    let allowed = if workspace.allowed_agents.is_empty() {
                        "all".to_string()
                    } else {
                        workspace.allowed_agents.len().to_string()
                    };
                    let default_agent = workspace.default_agent.as_deref().unwrap_or("-");
                    println!("{name}\t{}\t{} mounts\tallowed={allowed}\tdefault={default_agent}", workspace.workdir, workspace.mounts.len());
                }
                Ok(())
            }
            WorkspaceCommand::Show { name } => {
                let workspace = config
                    .workspaces
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?;
                println!("name: {name}");
                println!("workdir: {}", workspace.workdir);
                println!("allowed_agents: {}", if workspace.allowed_agents.is_empty() { "all".to_string() } else { workspace.allowed_agents.join(", ") });
                println!("default_agent: {}", workspace.default_agent.as_deref().unwrap_or("-"));
                println!("mounts:");
                for mount in &workspace.mounts {
                    let ro = if mount.readonly { " (ro)" } else { "" };
                    println!("  {} -> {}{ro}", mount.src, mount.dst);
                }
                Ok(())
            }
            WorkspaceCommand::Edit { name, workdir, mounts, remove_destinations, allowed_agents, remove_allowed_agents, default_agent, clear_default_agent } => {
                let upsert_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec(value))
                    .collect::<Result<Vec<_>>>()?;
                config.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        workdir,
                        upsert_mounts,
                        remove_destinations,
                        allowed_agents_to_add: allowed_agents,
                        allowed_agents_to_remove: remove_allowed_agents,
                        default_agent: if clear_default_agent { Some(None) } else { default_agent.map(Some) },
                    },
                )?;
                config.save(&paths)?;
                Ok(())
            }
            WorkspaceCommand::Remove { name } => {
                if config.remove_workspace(&name) {
                    config.save(&paths)?;
                }
                Ok(())
            }
        },
```

- [ ] **Step 5: Run the targeted tests and a smoke suite for config commands**

Run: `cargo nextest run -E 'test(parses_workspace_add_command | parses_workspace_edit_command | bootstrap_writes_default_agent_smith_entry | deserializes_saved_workspaces)'`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/lib.rs src/config.rs
git commit -m "feat: add workspace CLI commands"
```

---

### Task 3: Add Workspace-Aware `load` Grammar And Runtime Resolution

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/lib.rs`
- Modify: `src/workspace.rs`
- Modify: `src/runtime.rs`
- Test: `src/cli.rs`
- Test: `src/workspace.rs`
- Test: `src/runtime.rs`

- [ ] **Step 1: Write failing tests for new `load` grammar and workspace resolution**

Add these tests to `src/cli.rs`:

```rust
    #[test]
    fn parses_load_with_workspace_short_flag() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith", "-w", "big-monorepo"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load { workspace: Some(_), .. }
        ));
    }

    #[test]
    fn parses_load_with_custom_mounts() {
        let cli = Cli::try_parse_from([
            "jackin",
            "load",
            "agent-smith",
            "--mount",
            "/tmp/project:/workspace/project",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
            "--workdir",
            "/workspace/project",
        ]).unwrap();

        assert!(matches!(cli.command, Command::Load { .. }));
    }
```

Add this test module to `src/workspace.rs`:

```rust
    #[test]
    fn resolves_saved_workspace_and_rejects_disallowed_agent() {
        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource { git: "git@github.com:donbeave/jackin-agent-smith.git".to_string() },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            WorkspaceConfig {
                workdir: "/workspace/project".to_string(),
                mounts: vec![MountConfig {
                    src: "/tmp/project".to_string(),
                    dst: "/workspace/project".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
            },
        );

        let cwd = std::env::temp_dir();
        let error = resolve_load_workspace(
            &config,
            &crate::selector::ClassSelector::new(None, "neo"),
            &cwd,
            LoadWorkspaceInput::Saved("big-monorepo".to_string()),
        ).unwrap_err();

        assert!(error.to_string().contains("is not allowed by workspace"));
    }
```

- [ ] **Step 2: Run the targeted tests and confirm they fail**

Run: `cargo nextest run -E 'test(parses_load_with_workspace_short_flag | parses_load_with_custom_mounts | resolves_saved_workspace_and_rejects_disallowed_agent)'`

Expected: FAIL because the `Load` variant does not support those fields and `resolve_load_workspace` does not exist.

- [ ] **Step 3: Extend `Command::Load` and add `LoadWorkspaceInput`/`ResolvedWorkspace`**

Replace the `Load` variant in `src/cli.rs` with:

```rust
    Load {
        selector: String,
        #[arg(value_name = "PATH", conflicts_with_all = ["workspace", "mount", "workdir"])]
        path: Option<String>,
        #[arg(short = 'w', long = "workspace", conflicts_with_all = ["path", "mount", "workdir"])]
        workspace: Option<String>,
        #[arg(long = "mount", conflicts_with_all = ["path", "workspace"])]
        mounts: Vec<String>,
        #[arg(long, requires = "mounts", conflicts_with_all = ["path", "workspace"])]
        workdir: Option<String>,
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
```

In `src/workspace.rs`, add these runtime types and resolver:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWorkspaceInput {
    CurrentDir,
    Path(PathBuf),
    Saved(String),
    Custom { mounts: Vec<MountConfig>, workdir: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    pub label: String,
    pub workdir: String,
    pub mounts: Vec<MountConfig>,
}

pub fn resolve_load_workspace(
    config: &crate::config::AppConfig,
    selector: &crate::selector::ClassSelector,
    cwd: &Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<ResolvedWorkspace> {
    let workspace = match input {
        LoadWorkspaceInput::CurrentDir => current_dir_workspace(cwd)?,
        LoadWorkspaceInput::Path(path) => current_dir_workspace(&path)?,
        LoadWorkspaceInput::Saved(name) => {
            let workspace = config
                .workspaces
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?
                .clone();
            if !workspace.allowed_agents.is_empty() && !workspace.allowed_agents.iter().any(|agent| agent == &selector.key()) {
                anyhow::bail!("agent {} is not allowed by workspace {name}", selector.key());
            }
            workspace
        }
        LoadWorkspaceInput::Custom { mounts, workdir } => WorkspaceConfig {
            workdir,
            mounts,
            allowed_agents: vec![],
            default_agent: None,
        },
    };

    validate_workspace_config("runtime", &workspace)?;

    let mut mounts = workspace.mounts.clone();
    let global_mounts = config
        .resolve_mounts(selector)
        .into_iter()
        .map(|(_, mount)| mount)
        .collect::<Vec<_>>();
    validate_mounts(&global_mounts)?;

    for mount in global_mounts {
        if mounts.iter().any(|existing| existing.dst == mount.dst) {
            anyhow::bail!("global mount destination conflicts with workspace destination: {}", mount.dst);
        }
        mounts.push(mount);
    }

    Ok(ResolvedWorkspace {
        label: workspace.workdir.clone(),
        workdir: workspace.workdir,
        mounts,
    })
}
```

- [ ] **Step 4: Resolve workspace mode in `src/lib.rs` and pass it into `runtime::load_agent`**

Replace the `Command::Load` arm in `src/lib.rs` with:

```rust
        Command::Load { selector, path, workspace, mounts, workdir, no_intro, debug } => {
            let class = ClassSelector::parse(&selector)?;
            let cwd = std::env::current_dir()?;
            let workspace_input = if let Some(name) = workspace {
                crate::workspace::LoadWorkspaceInput::Saved(name)
            } else if !mounts.is_empty() {
                let mounts = mounts
                    .iter()
                    .map(|value| crate::workspace::parse_mount_spec(value))
                    .collect::<Result<Vec<_>>>()?;
                crate::workspace::LoadWorkspaceInput::Custom {
                    mounts,
                    workdir: workdir.ok_or_else(|| anyhow::anyhow!("--workdir is required when using --mount"))?,
                }
            } else if let Some(path) = path {
                crate::workspace::LoadWorkspaceInput::Path(std::path::PathBuf::from(path))
            } else {
                crate::workspace::LoadWorkspaceInput::CurrentDir
            };
            let resolved_workspace = crate::workspace::resolve_load_workspace(&config, &class, &cwd, workspace_input)?;
            let opts = runtime::LoadOptions { no_intro, debug };
            runtime::load_agent(&paths, &mut config, &class, &resolved_workspace, &mut runner, &opts)
        }
```

- [ ] **Step 5: Update `src/runtime.rs` to mount the resolved workspace instead of the cached repo checkout**

Change the signature of `load_agent`:

```rust
pub fn load_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &ClassSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
) -> anyhow::Result<()> {
```

Then replace the hard-coded repo mount in `run_args` with workspace mounts and `--workdir`:

```rust
        let mut run_args: Vec<String> = vec![
            "run".into(),
            "-it".into(),
            "--name".into(),
            container_name.clone(),
            "--hostname".into(),
            container_name.clone(),
            "--network".into(),
            network.clone(),
            "--label".into(),
            "jackin.managed=true".into(),
            "--label".into(),
            format!("jackin.class={}", selector.key()),
            "--workdir".into(),
            workspace.workdir.clone(),
            "-e".into(),
            format!("DOCKER_HOST=tcp://{dind}:2375"),
            "-v".into(),
            format!("{}:/home/claude/.claude", state.claude_dir.display()),
            "-v".into(),
            format!("{}:/home/claude/.claude.json", state.claude_json.display()),
            "-v".into(),
            format!("{}:/home/claude/.jackin/plugins.json:ro", state.plugins_json.display()),
        ];
        for mount in &workspace.mounts {
            let suffix = if mount.readonly { ":ro" } else { "" };
            run_args.extend([
                "-v".into(),
                format!("{}:{}{}", mount.src, mount.dst, suffix),
            ]);
        }
```

Delete the old `cached_repo.repo_dir -> /workspace` runtime mount.

- [ ] **Step 6: Add a runtime regression test for workspace mounts and workdir**

Add this test to `src/runtime.rs`:

```rust
    #[test]
    fn load_agent_uses_resolved_workspace_mounts_and_workdir() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(repo_dir.join("jackin.agent.toml"), "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n").unwrap();

        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).unwrap();
        let workspace = crate::workspace::ResolvedWorkspace {
            label: workspace_dir.display().to_string(),
            workdir: workspace_dir.display().to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: workspace_dir.display().to_string(),
                dst: workspace_dir.display().to_string(),
                readonly: false,
            }],
        };

        load_agent(&paths, &mut config, &selector, &workspace, &mut runner, &LoadOptions::default()).unwrap();

        let run_call = runner.recorded.iter().find(|call| call.contains("docker run -it")).unwrap();
        assert!(run_call.contains(&format!("--workdir {}", workspace.workdir)));
        assert!(run_call.contains(&format!("{}:{}", workspace_dir.display(), workspace_dir.display())));
        assert!(!run_call.contains(&format!("{}:/workspace", repo_dir.display())));
    }
```

- [ ] **Step 7: Run the targeted test set**

Run: `cargo nextest run -E 'test(parses_load_with_workspace_short_flag | parses_load_with_custom_mounts | resolves_saved_workspace_and_rejects_disallowed_agent | load_agent_uses_resolved_workspace_mounts_and_workdir)'`

Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/cli.rs src/lib.rs src/workspace.rs src/runtime.rs
git commit -m "feat: make load workspace-aware"
```

---

### Task 4: Align The Runtime User With The Host UID/GID

**Files:**
- Modify: `src/runtime.rs`
- Modify: `src/derived_image.rs`
- Test: `src/derived_image.rs`
- Test: `src/runtime.rs`

- [ ] **Step 1: Write failing tests for host UID/GID build args**

Add this test to `src/derived_image.rs`:

```rust
    #[test]
    fn renders_derived_dockerfile_rewrites_claude_uid_and_gid() {
        let dockerfile = render_derived_dockerfile("FROM donbeave/jackin-construct:trixie\n");
        assert!(dockerfile.contains("ARG JACKIN_HOST_UID=1000"));
        assert!(dockerfile.contains("ARG JACKIN_HOST_GID=1000"));
        assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" claude"));
        assert!(dockerfile.contains("usermod -o -u \"$JACKIN_HOST_UID\" -g \"$JACKIN_HOST_GID\" claude"));
    }
```

Add this test to `src/runtime.rs`:

```rust
    #[test]
    fn load_agent_passes_host_uid_and_gid_to_docker_build() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(repo_dir.join("jackin.agent.toml"), "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n").unwrap();

        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).unwrap();
        let workspace = crate::workspace::ResolvedWorkspace {
            label: workspace_dir.display().to_string(),
            workdir: workspace_dir.display().to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: workspace_dir.display().to_string(),
                dst: workspace_dir.display().to_string(),
                readonly: false,
            }],
        };

        load_agent(&paths, &mut config, &selector, &workspace, &mut runner, &LoadOptions::default()).unwrap();

        let build_call = runner.recorded.iter().find(|call| call.contains("docker build -t jackin-agent-smith")).unwrap();
        assert!(build_call.contains("--build-arg JACKIN_HOST_UID="));
        assert!(build_call.contains("--build-arg JACKIN_HOST_GID="));
    }
```

- [ ] **Step 2: Run the targeted tests and confirm they fail**

Run: `cargo nextest run -E 'test(renders_derived_dockerfile_rewrites_claude_uid_and_gid | load_agent_passes_host_uid_and_gid_to_docker_build)'`

Expected: FAIL because the derived Dockerfile and build command do not mention host UID/GID.

- [ ] **Step 3: Add host identity loading and pass build args from runtime**

In `src/runtime.rs`, add this helper alongside `GitIdentity`:

```rust
struct HostIdentity {
    uid: String,
    gid: String,
}

#[cfg(unix)]
fn load_host_identity() -> HostIdentity {
    let uid = std::process::Command::new("id")
        .args(["-u"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "1000".to_string());
    let gid = std::process::Command::new("id")
        .args(["-g"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "1000".to_string());
    HostIdentity { uid, gid }
}

#[cfg(not(unix))]
fn load_host_identity() -> HostIdentity {
    HostIdentity {
        uid: "1000".to_string(),
        gid: "1000".to_string(),
    }
}
```

Call it near the top of `load_agent` and add the build args:

```rust
    let host = load_host_identity();
```

```rust
        let build_args = [
            "build".into(),
            "--build-arg".into(),
            format!("JACKIN_HOST_UID={}", host.uid),
            "--build-arg".into(),
            format!("JACKIN_HOST_GID={}", host.gid),
            "-t".into(),
            image.clone(),
            "-f".into(),
            build.dockerfile_path.display().to_string(),
            build.context_dir.display().to_string(),
        ];
```

- [ ] **Step 4: Update the derived Dockerfile to rewrite the `claude` user**

In `src/derived_image.rs`, change `render_derived_dockerfile` to include UID/GID rewrite logic before switching back to `claude`:

```rust
pub fn render_derived_dockerfile(base_dockerfile: &str) -> String {
    format!(
        "{base_dockerfile}\nUSER root\nARG JACKIN_HOST_UID=1000\nARG JACKIN_HOST_GID=1000\nRUN current_gid=\"$(id -g claude)\" && if [ \"$current_gid\" != \"$JACKIN_HOST_GID\" ]; then groupmod -o -g \"$JACKIN_HOST_GID\" claude; fi && current_uid=\"$(id -u claude)\" && if [ \"$current_uid\" != \"$JACKIN_HOST_UID\" ]; then usermod -o -u \"$JACKIN_HOST_UID\" -g \"$JACKIN_HOST_GID\" claude; fi && chown -R claude:claude /home/claude\nUSER claude\nRUN curl -fsSL https://claude.ai/install.sh | bash\nRUN claude --version\nUSER root\nCOPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh\nRUN chmod +x /home/claude/entrypoint.sh\nWORKDIR /workspace\nUSER claude\nENTRYPOINT [\"/home/claude/entrypoint.sh\"]\n"
    )
}
```

- [ ] **Step 5: Run the targeted tests and a build-related smoke test**

Run: `cargo nextest run -E 'test(renders_derived_dockerfile_rewrites_claude_uid_and_gid | load_agent_passes_host_uid_and_gid_to_docker_build | creates_temp_context_with_repo_copy_and_runtime_assets)'`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/runtime.rs src/derived_image.rs
git commit -m "feat: align workspace runtime user with host uid"
```

---

### Task 5: Build The Interactive `launch` Picker With Ratatui

**Files:**
- Modify: `Cargo.toml`
- Create: `src/launch.rs`
- Modify: `src/cli.rs`
- Modify: `src/lib.rs`
- Test: `src/launch.rs`
- Test: `src/cli.rs`

- [ ] **Step 1: Add failing tests for launcher state behavior**

Create `src/launch.rs` with this initial test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preselects_saved_workspace_on_exact_workdir_match() {
        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource { git: "git@github.com:donbeave/jackin-agent-smith.git".to_string() },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: "/tmp/project".to_string(),
                mounts: vec![crate::workspace::MountConfig {
                    src: "/tmp/project".to_string(),
                    dst: "/tmp/project".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
            },
        );

        let state = LaunchState::new(&config, std::path::Path::new("/tmp/project")).unwrap();
        assert_eq!(state.selected_workspace_name(), Some("big-monorepo"));
    }

    #[test]
    fn filters_agents_by_query() {
        let state = LaunchState {
            stage: LaunchStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: "chainargos".to_string(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![
                    crate::selector::ClassSelector::new(None, "agent-smith"),
                    crate::selector::ClassSelector::new(Some("chainargos"), "the-architect"),
                ],
                default_agent: None,
            }],
        };

        let filtered = state.filtered_agents();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key(), "chainargos/the-architect");
    }
}
```

- [ ] **Step 2: Run the targeted tests and confirm they fail**

Run: `cargo nextest run -E 'test(preselects_saved_workspace_on_exact_workdir_match | filters_agents_by_query)'`

Expected: FAIL because `src/launch.rs` and the state types do not exist.

- [ ] **Step 3: Add Ratatui dependencies and the `launch` CLI command**

In `Cargo.toml`, add:

```toml
ratatui = "0.29"
crossterm = "0.28"
```

In `src/cli.rs`, add:

```rust
    /// Fast interactive launcher
    Launch,
```

and a parsing test:

```rust
    #[test]
    fn parses_launch_command() {
        let cli = Cli::try_parse_from(["jackin", "launch"]).unwrap();
        assert!(matches!(cli.command, Command::Launch));
    }
```

- [ ] **Step 4: Implement the pure launcher state in `src/launch.rs`**

Create `src/launch.rs` with these core types first:

```rust
use crate::config::AppConfig;
use crate::selector::ClassSelector;
use crate::workspace::{current_dir_workspace, ResolvedWorkspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStage {
    Workspace,
    Agent,
}

#[derive(Debug, Clone)]
pub struct WorkspaceChoice {
    pub name: String,
    pub workspace: ResolvedWorkspace,
    pub allowed_agents: Vec<ClassSelector>,
    pub default_agent: Option<String>,
    pub global_mounts: Vec<crate::workspace::MountConfig>,
}

#[derive(Debug, Clone)]
pub struct LaunchState {
    pub stage: LaunchStage,
    pub selected_workspace: usize,
    pub selected_agent: usize,
    pub agent_query: String,
    pub workspaces: Vec<WorkspaceChoice>,
}

impl LaunchState {
    pub fn new(config: &AppConfig, cwd: &std::path::Path) -> anyhow::Result<Self> {
        let current = current_dir_workspace(cwd)?;
        let global_mounts = config.global_mounts();
        let current_choice = WorkspaceChoice {
            name: "Current directory".to_string(),
            workspace: ResolvedWorkspace {
                label: current.workdir.clone(),
                workdir: current.workdir.clone(),
                mounts: current.mounts.clone(),
            },
            allowed_agents: configured_agents(config),
            default_agent: None,
            global_mounts: global_mounts.clone(),
        };

        let mut workspaces = vec![current_choice];
        for (name, saved) in &config.workspaces {
            let allowed_agents = eligible_agents_for_saved_workspace(config, saved);
            workspaces.push(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                },
                allowed_agents,
                default_agent: saved.default_agent.clone(),
                global_mounts: global_mounts.clone(),
            });
        }

        let selected_workspace = workspaces
            .iter()
            .position(|choice| choice.name != "Current directory" && choice.workspace.workdir == cwd.display().to_string())
            .unwrap_or(0);

        Ok(Self {
            stage: LaunchStage::Workspace,
            selected_workspace,
            selected_agent: 0,
            agent_query: String::new(),
            workspaces,
        })
    }

    pub fn selected_workspace_name(&self) -> Option<&str> {
        self.workspaces.get(self.selected_workspace).map(|choice| choice.name.as_str())
    }

    pub fn filtered_agents(&self) -> Vec<ClassSelector> {
        let query = self.agent_query.to_ascii_lowercase();
        self.workspaces[self.selected_workspace]
            .allowed_agents
            .iter()
            .filter(|agent| query.is_empty() || agent.key().to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    }
}

fn configured_agents(config: &AppConfig) -> Vec<ClassSelector> {
    config
        .agents
        .keys()
        .filter_map(|key| ClassSelector::parse(key).ok())
        .collect()
}

fn eligible_agents_for_saved_workspace(config: &AppConfig, workspace: &crate::workspace::WorkspaceConfig) -> Vec<ClassSelector> {
    configured_agents(config)
        .into_iter()
        .filter(|agent| workspace.allowed_agents.is_empty() || workspace.allowed_agents.iter().any(|allowed| allowed == &agent.key()))
        .collect()
}
```

- [ ] **Step 5: Add the Ratatui render/input loop and wire `Command::Launch` in `src/lib.rs`**

Add a public entrypoint to `src/launch.rs` that returns a selected agent/workspace pair:

```rust
pub fn run_launch(config: &AppConfig, cwd: &std::path::Path) -> anyhow::Result<(ClassSelector, ResolvedWorkspace)> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
    use crossterm::ExecutableCommand;
    use ratatui::prelude::*;
    use ratatui::widgets::*;

    let mut state = LaunchState::new(config, cwd)?;
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = loop {
        terminal.draw(|frame| draw_launch(frame, &state))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match state.stage {
                LaunchStage::Workspace => match key.code {
                    KeyCode::Up => state.selected_workspace = state.selected_workspace.saturating_sub(1),
                    KeyCode::Down => state.selected_workspace = (state.selected_workspace + 1).min(state.workspaces.len().saturating_sub(1)),
                    KeyCode::Enter => {
                        let agents = state.filtered_agents();
                        if agents.len() == 1 {
                            break Ok((agents[0].clone(), state.workspaces[state.selected_workspace].workspace.clone()));
                        }
                        state.stage = LaunchStage::Agent;
                        state.agent_query.clear();
                        state.selected_agent = 0;
                    }
                    KeyCode::Char('q') | KeyCode::Esc => break Err(anyhow::anyhow!("launch cancelled")),
                    _ => {}
                },
                LaunchStage::Agent => match key.code {
                    KeyCode::Esc => {
                        state.stage = LaunchStage::Workspace;
                        state.agent_query.clear();
                        state.selected_agent = 0;
                    }
                    KeyCode::Backspace => {
                        state.agent_query.pop();
                        state.selected_agent = 0;
                    }
                    KeyCode::Char(ch) => {
                        state.agent_query.push(ch);
                        state.selected_agent = 0;
                    }
                    KeyCode::Up => state.selected_agent = state.selected_agent.saturating_sub(1),
                    KeyCode::Down => state.selected_agent = (state.selected_agent + 1).min(state.filtered_agents().len().saturating_sub(1)),
                    KeyCode::Enter => {
                        let agents = state.filtered_agents();
                        let agent = agents.get(state.selected_agent).ok_or_else(|| anyhow::anyhow!("no agent selected"))?;
                        break Ok((agent.clone(), state.workspaces[state.selected_workspace].workspace.clone()));
                    }
                    _ => {}
                },
            }
        }
    };

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}
```

Add this `draw_launch` helper in the same file so the launcher has a concrete, testable layout:

```rust
fn draw_launch(frame: &mut ratatui::Frame, state: &LaunchState) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(8), Constraint::Length(2)])
        .split(frame.area());
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(root[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Min(8)])
        .split(body[1]);

    let workspace_items = state
        .workspaces
        .iter()
        .map(|workspace| ListItem::new(workspace.name.clone()))
        .collect::<Vec<_>>();
    let workspace_list = List::new(workspace_items)
        .block(Block::default().title("Workspaces").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut workspace_state = ratatui::widgets::ListState::default();
    workspace_state.select(Some(state.selected_workspace));
    frame.render_stateful_widget(workspace_list, body[0], &mut workspace_state);

    let selected_workspace = &state.workspaces[state.selected_workspace];
    let mount_lines = selected_workspace
        .workspace
        .mounts
        .iter()
        .map(|mount| {
            let ro = if mount.readonly { " (ro)" } else { "" };
            format!("{} -> {}{}", mount.src, mount.dst, ro)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let global_lines = selected_workspace
        .global_mounts
        .iter()
        .map(|mount| {
            let ro = if mount.readonly { " (ro)" } else { "" };
            format!("{} -> {}{}", mount.src, mount.dst, ro)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let details = Paragraph::new(format!(
        "available agents: {}\nworkdir: {}\n\nmounts:\n{}\n\nglobal:\n{}",
        selected_workspace.allowed_agents.len(),
        selected_workspace.workspace.workdir,
        mount_lines,
        global_lines,
    ))
    .block(Block::default().title("Workspace Details").borders(Borders::ALL));
    frame.render_widget(details, right[0]);

    let agent_items = state
        .filtered_agents()
        .into_iter()
        .map(|agent| ListItem::new(agent.key()))
        .collect::<Vec<_>>();
    let agent_title = if state.stage == LaunchStage::Agent {
        format!("Agents (filter: {})", state.agent_query)
    } else {
        "Agents".to_string()
    };
    let agent_list = List::new(agent_items)
        .block(Block::default().title(agent_title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut agent_state = ratatui::widgets::ListState::default();
    agent_state.select(Some(state.selected_agent));
    frame.render_stateful_widget(agent_list, right[1], &mut agent_state);

    let footer = Paragraph::new("Enter select   Esc back   q quit   Type to filter agents")
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, root[2]);
}
```

Then in `src/lib.rs`, add:

```rust
pub mod launch;
```

and route the new command:

```rust
        Command::Launch => {
            let cwd = std::env::current_dir()?;
            let (class, workspace) = crate::launch::run_launch(&config, &cwd)?;
            let opts = runtime::LoadOptions { no_intro: false, debug: false };
            runtime::load_agent(&paths, &mut config, &class, &workspace, &mut runner, &opts)
        }
```

- [ ] **Step 6: Run the launcher-focused tests and a compile check**

Run: `cargo nextest run -E 'test(preselects_saved_workspace_on_exact_workdir_match | filters_agents_by_query | parses_launch_command)'`

Expected: PASS

Run: `cargo check`

Expected: compiles with `ratatui` and `crossterm` linked successfully

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/launch.rs src/cli.rs src/lib.rs
git commit -m "feat: add interactive launch workspace picker"
```

---

### Task 6: Update README And Run Full Verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the command overview in `README.md`**

Replace the command list in `README.md` with:

```md
- `jackin launch` — fast interactive launcher for the current directory or a saved workspace.
- `jackin load agent-smith` — send an agent in using the current directory as the workspace.
- `jackin load agent-smith ~/Projects/chainargos/big-monorepo` — send an agent into a direct path workspace.
- `jackin load agent-smith -w big-monorepo` — use a saved workspace definition.
- `jackin hardline jackin-agent-smith` — reattach to a running agent.
- `jackin eject jackin-agent-smith` — pull one agent out.
- `jackin workspace add big-monorepo --workdir /workspace/project --mount ~/Projects/chainargos/big-monorepo:/workspace/project` — save a reusable workspace.
```

- [ ] **Step 2: Add a new `Workspaces` section to `README.md`**

Insert this section after `## Storage`:

```md
## Workspaces

`jackin launch` is the fastest way to start work. It shows two kinds of workspace choices:

- `Current directory` — a synthetic workspace that mounts the current directory to the same absolute path inside the container and uses that path as `workdir`
- saved workspaces — named local definitions stored in `~/.config/jackin/config.toml`

If the current directory exactly matches a saved workspace `workdir`, Jackin preselects that saved workspace in the launcher. You can still move to `Current directory` to force the raw direct-mount behavior.

Saved workspaces are local operator config. They define mounts, `workdir`, and optional allowed/default agents.
```

- [ ] **Step 3: Update the runtime wiring paragraph to reflect workspace mounts**

Replace this sentence:

```md
`agent-smith`-style agent repos only own their agent-specific environment layer. `jackin` owns the runtime wiring around that layer: validating the repo contract, generating the derived Dockerfile, installing Claude into the derived image, injecting the runtime entrypoint, mounting the cached repo checkout at `/workspace`, mounting persisted `.claude`, `.claude.json`, and `plugins.json`, and wiring the per-agent Docker-in-Docker runtime.
```

with:

```md
`agent-smith`-style agent repos only own their agent-specific environment layer. `jackin` owns the runtime wiring around that layer: validating the repo contract, generating the derived Dockerfile, installing Claude into the derived image, injecting the runtime entrypoint, mounting the resolved workspace paths into the runtime container, mounting persisted `.claude`, `.claude.json`, and `plugins.json`, and wiring the per-agent Docker-in-Docker runtime.
```

- [ ] **Step 4: Run the full verification suite**

Run: `cargo nextest run`

Expected: PASS across the full Rust test suite

Run: `cargo check`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document launch and workspace flows"
```

---

## Self-Review

- Spec coverage: the tasks cover saved workspace persistence, workspace CLI, load grammar, current-directory runtime behavior, launcher TUI, host UID/GID alignment, and README updates.
- Placeholder scan: no `TODO`, `TBD`, or hand-wavy “add validation” steps remain; each task includes concrete code or exact commands.
- Type consistency: `MountConfig`, `WorkspaceConfig`, `WorkspaceEdit`, `LoadWorkspaceInput`, `ResolvedWorkspace`, `LaunchState`, and `WorkspaceCommand` names are used consistently across tasks.
