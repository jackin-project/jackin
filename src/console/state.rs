use std::cell::RefCell;
use std::rc::Rc;

use crate::app::context::{eligible_agents_for_workspace, find_saved_workspace_for_cwd};
use crate::config::{AppConfig, MountEntry};
use crate::console::op_cache::OpCache;
use crate::selector::ClassSelector;
use crate::workspace::{LoadWorkspaceInput, MountConfig, ResolvedWorkspace, current_dir_workspace};

/// Top-level stage of the operator console.
///
/// Single-variant today — the legacy full-screen `Agent` picker was
/// replaced by `Modal::AgentPicker` overlaid on the manager list. Kept
/// as an `enum` so future stages (e.g. running-sessions cluster) can
/// land without rewriting every `ConsoleStage::Manager(_)` match site.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConsoleStage {
    Manager(crate::console::manager::ManagerState<'static>),
}

#[derive(Debug, Clone)]
pub struct WorkspaceChoice {
    pub name: String,
    pub workspace: ResolvedWorkspace,
    pub allowed_agents: Vec<ClassSelector>,
    pub default_agent: Option<String>,
    pub last_agent: Option<String>,
    pub global_mounts: Vec<MountConfig>,
    pub input: LoadWorkspaceInput,
}

#[derive(Debug)]
pub struct ConsoleState {
    pub stage: ConsoleStage,
    /// Which entry in `workspaces` the operator is launching against.
    /// Pinned by the launch dispatcher when the operator presses Enter
    /// on a manager row, then read back by the `AgentPicker` commit
    /// path to resolve the chosen agent against the right workspace.
    pub selected_workspace: usize,
    pub workspaces: Vec<WorkspaceChoice>,
    /// Process-lifetime cache of `op` structural metadata, shared with
    /// the embedded `ManagerState` and any picker the operator opens.
    /// Survives Esc-back-to-list and editor re-entry within a single
    /// `jackin console` invocation. See [`OpCache`].
    ///
    /// `Rc<RefCell<_>>` so the picker can hold a clone of the handle
    /// while the modal is open. The TUI event loop is single-threaded;
    /// `RefCell` is sufficient — no `Mutex` needed.
    pub op_cache: Rc<RefCell<OpCache>>,
}

impl ConsoleState {
    pub fn new(config: &AppConfig, cwd: &std::path::Path) -> anyhow::Result<Self> {
        let current = current_dir_workspace(cwd)?;
        let global_mounts = global_mounts(config)?;
        let current_choice = WorkspaceChoice {
            name: "Current directory".to_string(),
            workspace: ResolvedWorkspace {
                label: current.workdir.clone(),
                workdir: current.workdir,
                mounts: current.mounts,
            },
            allowed_agents: configured_agents(config),
            default_agent: None,
            last_agent: None,
            global_mounts: global_mounts.clone(),
            input: LoadWorkspaceInput::CurrentDir,
        };

        let mut workspaces = vec![current_choice];
        for (name, saved) in &config.workspaces {
            let allowed_agents = eligible_agents_for_workspace(config, saved);
            workspaces.push(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                },
                allowed_agents,
                default_agent: saved.default_agent.clone(),
                last_agent: saved.last_agent.clone(),
                global_mounts: global_mounts.clone(),
                input: LoadWorkspaceInput::Saved(name.clone()),
            });
        }

        // Preselect the saved workspace that best covers `cwd`. The
        // decision uses the shared helper in `app::context` so the TUI
        // and the non-interactive CLI agree on "which workspace am I in?".
        // Falls back to index 0 (the synthetic "Current directory" choice)
        // if no saved workspace matches.
        let selected_workspace = find_saved_workspace_for_cwd(config, cwd)
            .and_then(|(name, _)| workspaces.iter().position(|choice| choice.name == name))
            .unwrap_or(0);

        let op_cache = Rc::new(RefCell::new(OpCache::default()));
        Ok(Self {
            stage: ConsoleStage::Manager(
                crate::console::manager::ManagerState::from_config_with_cache(
                    config,
                    cwd,
                    op_cache.clone(),
                ),
            ),
            selected_workspace,
            workspaces,
            op_cache,
        })
    }

    pub fn selected_workspace_name(&self) -> Option<&str> {
        self.workspaces
            .get(self.selected_workspace)
            .map(|choice| choice.name.as_str())
    }
}

fn configured_agents(config: &AppConfig) -> Vec<ClassSelector> {
    config
        .agents
        .keys()
        .filter_map(|key| ClassSelector::parse(key).ok())
        .collect()
}

fn global_mounts(config: &AppConfig) -> anyhow::Result<Vec<MountConfig>> {
    let mounts = config
        .docker
        .mounts
        .iter()
        .filter_map(|(name, entry)| match entry {
            MountEntry::Mount(mount) => Some((name.clone(), mount.clone())),
            MountEntry::Scoped(_) => None,
        })
        .collect::<Vec<_>>();

    AppConfig::expand_and_validate_named_mounts(&mounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preselects_saved_workspace_on_exact_workdir_match() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().canonicalize().unwrap();
        let workdir = project_dir.display().to_string();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: workdir.clone(),
                mounts: vec![crate::workspace::MountConfig {
                    src: workdir.clone(),
                    dst: workdir,
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        );

        let state = ConsoleState::new(&config, &project_dir).unwrap();
        assert_eq!(state.selected_workspace_name(), Some("big-monorepo"));
    }

    #[test]
    fn preselects_saved_workspace_for_nested_directory_under_mount_root() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src/lib");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let nested_dir = nested_dir.canonicalize().unwrap();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![crate::workspace::MountConfig {
                    src: project_dir.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        );

        let state = ConsoleState::new(&config, &nested_dir).unwrap();
        assert_eq!(state.selected_workspace_name(), Some("big-monorepo"));
    }

    #[test]
    fn preselects_saved_workspace_from_host_workdir_root() {
        let temp = tempfile::tempdir().unwrap();
        let workspace_root = temp.path().join("monorepo");
        let repo_dir = workspace_root.join("jackin");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let workspace_root = workspace_root.canonicalize().unwrap();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: workspace_root.display().to_string(),
                mounts: vec![crate::workspace::MountConfig {
                    src: repo_dir.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        );

        let state = ConsoleState::new(&config, &workspace_root).unwrap();
        assert_eq!(state.selected_workspace_name(), Some("big-monorepo"));
    }

    // ── Phase 0 gap-fill: agent-eligibility composition ────────────────────
    //
    // These tests pin the composition the TUI relies on:
    //
    //   configured_agents  →  eligible_agents_for_workspace
    //                     (allowed_agents filter)  →
    //                     workspace.allowed_agents  →
    //                     on-screen result
    //
    // Invariants:
    //
    //   1. An empty `allowed_agents` list means "any configured agent."
    //   2. A non-empty `allowed_agents` list strictly narrows to the named
    //      set, and never resurrects an unconfigured ("ghost") name.
    //
    // Filter-time narrowing now lives on `AgentPickerState` (see
    // `widgets/agent_picker.rs`) — the legacy `ConsoleState::filtered_agents`
    // path was deleted alongside the full-screen `ConsoleStage::Agent`.

    fn agent_source_stub() -> crate::config::AgentSource {
        crate::config::AgentSource {
            git: "https://example.invalid/org/repo.git".to_string(),
            trusted: true,
            claude: None,
            env: std::collections::BTreeMap::new(),
        }
    }

    fn workspace_with_allowed(allowed: &[&str]) -> crate::workspace::WorkspaceConfig {
        crate::workspace::WorkspaceConfig {
            workdir: "/work".to_string(),
            mounts: vec![],
            allowed_agents: allowed.iter().map(|s| (*s).to_string()).collect(),
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn eligible_agents_returns_all_configured_when_allowed_list_empty() {
        let mut config = crate::config::AppConfig::default();
        config
            .agents
            .insert("alice".to_string(), agent_source_stub());
        config.agents.insert("bob".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&[]);
        let eligible = eligible_agents_for_workspace(&config, &ws);
        let keys: Vec<String> = eligible
            .iter()
            .map(crate::selector::ClassSelector::key)
            .collect();

        assert_eq!(eligible.len(), 2, "empty allowed_agents must mean 'any'");
        assert!(keys.contains(&"alice".to_string()));
        assert!(keys.contains(&"bob".to_string()));
    }

    #[test]
    fn eligible_agents_narrows_to_allowed_list_when_non_empty() {
        let mut config = crate::config::AppConfig::default();
        config
            .agents
            .insert("alice".to_string(), agent_source_stub());
        config.agents.insert("bob".to_string(), agent_source_stub());
        config
            .agents
            .insert("carol".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&["alice", "carol"]);
        let eligible = eligible_agents_for_workspace(&config, &ws);
        let keys: Vec<String> = eligible
            .iter()
            .map(crate::selector::ClassSelector::key)
            .collect();

        assert_eq!(eligible.len(), 2);
        assert!(keys.contains(&"alice".to_string()));
        assert!(keys.contains(&"carol".to_string()));
        assert!(!keys.contains(&"bob".to_string()));
    }

    #[test]
    fn eligible_agents_drops_ghost_name_not_in_config() {
        // `allowed_agents` references an agent that was removed from config.
        // The eligibility set must not fabricate a selector for it.
        let mut config = crate::config::AppConfig::default();
        config
            .agents
            .insert("alice".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&["ghost"]);
        let eligible = eligible_agents_for_workspace(&config, &ws);

        assert!(
            eligible.is_empty(),
            "eligibility must not resurrect a name absent from config.agents"
        );
    }
}
