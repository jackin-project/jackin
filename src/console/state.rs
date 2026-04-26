use std::cell::RefCell;
use std::rc::Rc;

use crate::app::context::eligible_agents_for_workspace;
use crate::config::{AppConfig, MountEntry};
use crate::console::op_cache::OpCache;
use crate::selector::ClassSelector;
use crate::workspace::{LoadWorkspaceInput, MountConfig, ResolvedWorkspace, current_dir_workspace};

/// Single-variant today; kept as `enum` so future stages (e.g.
/// running-sessions cluster) land without churning every match site.
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
    /// `LoadWorkspaceInput` (not an index) so each dispatch rebuilds
    /// its `WorkspaceChoice` from current config — manager edits flow
    /// through immediately.
    pub pending_launch: Option<LoadWorkspaceInput>,
    /// Process-lifetime `op` metadata cache. `Rc<RefCell<_>>` because
    /// the TUI event loop is single-threaded.
    pub op_cache: Rc<RefCell<OpCache>>,
    /// Probed once at startup; mid-session installs require restart.
    /// Re-probing on every modal open would add a perceptible hitch.
    pub op_available: bool,
    /// Lifted to `ConsoleState` (not `ManagerState`) so it overlays
    /// any sub-stage uniformly.
    pub quit_confirm: Option<crate::console::widgets::confirm::ConfirmState>,
}

impl ConsoleState {
    pub fn new(config: &AppConfig, cwd: &std::path::Path) -> anyhow::Result<Self> {
        let op_cache = Rc::new(RefCell::new(OpCache::default()));
        let op_available = {
            use crate::operator_env::OpRunner as _;
            crate::operator_env::OpCli::new_probe().probe().is_ok()
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

/// `Ok(None)` when a saved name went missing between keypress and
/// dispatch (concurrent delete via the manager).
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
        // CLI-only shape (`jackin load --path`); console never
        // produces it.
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
            MountEntry::Mount(mount) => Some((name.clone(), MountConfig::from(mount.clone()))),
            MountEntry::Scoped(_) => None,
        })
        .collect::<Vec<_>>();

    AppConfig::expand_and_validate_named_mounts(&mounts)
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    isolation: crate::isolation::MountIsolation::Shared,
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

    // ── agent-eligibility composition ───────────────────────────────

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
