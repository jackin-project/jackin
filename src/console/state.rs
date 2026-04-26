use std::cell::RefCell;
use std::rc::Rc;

use crate::app::context::eligible_agents_for_workspace;
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
    /// Workspace whose `AgentPicker` is currently open (or was just dispatched
    /// against). Pinned by `dispatch_launch_for_workspace` when it routes
    /// to Branch 3 (multiple eligible agents → picker), then read back by
    /// the `LaunchWithAgent` arm in `run_console` to rebuild a fresh
    /// `WorkspaceChoice` on commit.
    ///
    /// Storing a `LoadWorkspaceInput` rather than an index decouples
    /// launch routing from any cached snapshot of the workspace list.
    /// Each launch attempt rebuilds its `WorkspaceChoice` from the
    /// current `AppConfig` via [`build_workspace_choice`] so manager
    /// edits (rename / create / delete / `default_agent` / env) take
    /// effect immediately — no stale-snapshot bug. See PR #171 review:
    /// commit 53.
    pub pending_launch: Option<LoadWorkspaceInput>,
    /// Process-lifetime cache of `op` structural metadata, shared with
    /// the embedded `ManagerState` and any picker the operator opens.
    /// Survives Esc-back-to-list and editor re-entry within a single
    /// `jackin console` invocation. See [`OpCache`].
    ///
    /// `Rc<RefCell<_>>` so the picker can hold a clone of the handle
    /// while the modal is open. The TUI event loop is single-threaded;
    /// `RefCell` is sufficient — no `Mutex` needed.
    pub op_cache: Rc<RefCell<OpCache>>,
    /// Whether the 1Password CLI (`op`) was reachable on PATH at console
    /// startup. Probed exactly once via [`OpCli::probe`] in
    /// [`ConsoleState::new`]; the result is read by the Secrets-tab
    /// source-picker modal to decide whether the `1Password` button is
    /// available or rendered dim.
    ///
    /// **Mid-session installs are not picked up.** If the operator
    /// installs `op` after `jackin console` has started, they must
    /// restart the console for the source-picker to enable the
    /// 1Password choice. The probe is a synchronous subprocess spawn;
    /// running it on every modal open would add a perceptible UI hitch
    /// for negligible benefit.
    pub op_available: bool,
    /// Top-level "Exit jackin'?" confirmation dialog. Opened by `Q`
    /// pressed anywhere outside the manager list (with no `list_modal`),
    /// closed by Y (commits an exit) / N / Esc (returns the operator
    /// to wherever they were). Lifted to `ConsoleState` rather than
    /// `ManagerState` because the dialog must overlay any sub-stage
    /// uniformly — see `is_on_main_screen` / `consumes_letter_input`
    /// in `console::mod` for the routing gate.
    pub quit_confirm: Option<crate::console::widgets::confirm::ConfirmState>,
}

impl ConsoleState {
    pub fn new(config: &AppConfig, cwd: &std::path::Path) -> anyhow::Result<Self> {
        let op_cache = Rc::new(RefCell::new(OpCache::default()));
        // One-shot `op --version` probe — same code path the launch-time
        // resolver uses. Failure is fine: it just means the source
        // picker's 1Password choice will render disabled. Probe runs
        // exactly once per console invocation; mid-session installs
        // require a restart (see `op_available` doc).
        let op_available = {
            use crate::operator_env::OpRunner as _;
            crate::operator_env::OpCli::new().probe().is_ok()
        };
        Ok(Self {
            stage: ConsoleStage::Manager(
                crate::console::manager::ManagerState::from_config_with_cache_and_op(
                    config,
                    cwd,
                    op_cache.clone(),
                    op_available,
                ),
            ),
            pending_launch: None,
            op_cache,
            op_available,
            quit_confirm: None,
        })
    }
}

/// Build a fresh [`WorkspaceChoice`] from the current `AppConfig` for `input`.
///
/// `input` is a saved workspace name or the synthetic "current directory"
/// choice. Called at launch dispatch time so manager edits — create,
/// rename, delete, `default_agent`, `allowed_agents`, env — flow through
/// immediately.
///
/// Returns `Ok(None)` when `input` is `Saved(name)` but `name` is no
/// longer present in `config.workspaces` (e.g. the operator deleted it
/// via the manager between the keypress and the dispatch). Surfaces
/// resolution errors verbatim — they bubble up as a hard error from
/// `dispatch_launch_for_workspace`.
///
/// Replaces the old `ConsoleState.workspaces: Vec<WorkspaceChoice>`
/// snapshot built once at console startup. See `pending_launch` for the
/// motivation.
pub fn build_workspace_choice(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: &LoadWorkspaceInput,
) -> anyhow::Result<Option<WorkspaceChoice>> {
    let global_mounts = global_mounts(config)?;
    match input {
        LoadWorkspaceInput::CurrentDir => {
            let current = current_dir_workspace(cwd)?;
            Ok(Some(WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: ResolvedWorkspace {
                    label: current.workdir.clone(),
                    workdir: current.workdir,
                    mounts: current.mounts,
                },
                allowed_agents: configured_agents(config),
                default_agent: None,
                last_agent: None,
                global_mounts,
                input: LoadWorkspaceInput::CurrentDir,
            }))
        }
        LoadWorkspaceInput::Saved(name) => {
            let Some(saved) = config.workspaces.get(name) else {
                return Ok(None);
            };
            let allowed_agents = eligible_agents_for_workspace(config, saved);
            Ok(Some(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                },
                allowed_agents,
                default_agent: saved.default_agent.clone(),
                last_agent: saved.last_agent.clone(),
                global_mounts,
                input: LoadWorkspaceInput::Saved(name.clone()),
            }))
        }
        // `Path { .. }` is a CLI-only shape (`jackin load --path`); the
        // console never produces it. Reject it loudly rather than papering
        // over an unexpected dispatcher input.
        LoadWorkspaceInput::Path { .. } => Ok(None),
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

    // Preselection of the saved workspace covering `cwd` is pinned by
    // `ManagerState::from_config`'s tests in
    // `console::manager::state::tests::manager_preselects_saved_workspace_matching_cwd`.
    // The old `selected_workspace_name` accessor on `ConsoleState` was
    // removed in commit 53 (PR #171) when the snapshot list was dropped
    // in favour of build-on-demand `WorkspaceChoice`s — see
    // [`build_workspace_choice`].

    // ── build_workspace_choice: derive launch routing from current config ──
    //
    // These tests pin the new model: each launch dispatch builds a
    // fresh `WorkspaceChoice` from the current `AppConfig`, so manager
    // edits flow through immediately. Regression coverage for the
    // stale-snapshot bug fixed in commit 53 lives in
    // `tests/manager_flow.rs::launch_after_*`.

    #[test]
    fn build_workspace_choice_returns_none_for_unknown_saved_name() {
        let config = crate::config::AppConfig::default();
        let cwd = std::env::temp_dir();
        let result =
            build_workspace_choice(&config, &cwd, &LoadWorkspaceInput::Saved("ghost".into()))
                .unwrap();
        assert!(
            result.is_none(),
            "Saved(name) for an absent workspace must return None, not fabricate a choice"
        );
    }

    #[test]
    fn build_workspace_choice_picks_up_default_agent_from_config() {
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
            "ws".to_string(),
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

        let choice = build_workspace_choice(
            &config,
            &project_dir,
            &LoadWorkspaceInput::Saved("ws".into()),
        )
        .unwrap()
        .expect("present saved workspace must resolve");
        assert_eq!(choice.default_agent.as_deref(), Some("agent-smith"));
        assert_eq!(choice.allowed_agents.len(), 1);
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
