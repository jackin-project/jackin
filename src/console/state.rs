use crate::app::context::{eligible_agents_for_workspace, find_saved_workspace_for_cwd};
use crate::config::{AppConfig, MountEntry};
use crate::selector::ClassSelector;
use crate::workspace::{LoadWorkspaceInput, MountConfig, ResolvedWorkspace, current_dir_workspace};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConsoleStage {
    Manager(crate::console::manager::ManagerState<'static>),
    Agent,
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
    pub selected_workspace: usize,
    pub selected_agent: usize,
    pub agent_query: String,
    pub workspaces: Vec<WorkspaceChoice>,
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

        Ok(Self {
            stage: ConsoleStage::Manager(crate::console::manager::ManagerState::from_config(
                config, cwd,
            )),
            selected_workspace,
            selected_agent: 0,
            agent_query: String::new(),
            workspaces,
        })
    }

    pub fn selected_workspace_name(&self) -> Option<&str> {
        self.workspaces
            .get(self.selected_workspace)
            .map(|choice| choice.name.as_str())
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

    #[test]
    fn filters_agents_by_query() {
        let state = ConsoleState {
            stage: ConsoleStage::Agent,
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
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        let filtered = state.filtered_agents();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key(), "chainargos/the-architect");
    }

    // ── Phase 0 gap-fill: agent-filter composition ─────────────────────────
    //
    // These tests pin the composition the TUI relies on:
    //
    //   configured_agents  →  eligible_agents_for_workspace
    //                     (allowed_agents filter)  →
    //                     workspace.allowed_agents  →
    //                     filtered_agents          (agent_query filter)  →
    //                     on-screen result
    //
    // Invariants the plan's Phase 0 calls out for the Phase 6 unification:
    //
    //   1. An empty `allowed_agents` list means "any configured agent."
    //   2. A non-empty `allowed_agents` list strictly narrows to the named
    //      set, and never resurrects an unconfigured ("ghost") name.
    //   3. The query filter composes with — never widens — the post-eligibility
    //      set. A key not in `workspace.allowed_agents` cannot be recovered
    //      by any query string.
    //   4. An empty query returns the full post-eligibility set.
    //   5. A query that matches a subset of the eligible set returns exactly
    //      that subset (does not drop matches, does not add non-matches).

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

    #[test]
    fn empty_query_returns_full_post_eligibility_set() {
        let state = ConsoleState {
            stage: ConsoleStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: String::new(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![
                    crate::selector::ClassSelector::new(None, "alice"),
                    crate::selector::ClassSelector::new(None, "bob"),
                ],
                default_agent: None,
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        let filtered = state.filtered_agents();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn query_cannot_reintroduce_agent_excluded_by_allowed_list() {
        // `state.workspaces[_].allowed_agents` already reflects the
        // eligibility filter. An agent absent here cannot be resurrected
        // by *any* query string — the query only narrows, never widens.
        let state = ConsoleState {
            stage: ConsoleStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: "bob".to_string(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![crate::selector::ClassSelector::new(None, "alice")],
                default_agent: None,
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        assert!(
            state.filtered_agents().is_empty(),
            "query must not resurrect an excluded agent"
        );
    }

    #[test]
    fn query_narrows_within_allowed_set_without_dropping_matches() {
        // Multiple eligible agents; query matches a subset. Every matching
        // agent must still appear; no non-matching agent may sneak through.
        let state = ConsoleState {
            stage: ConsoleStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: "smith".to_string(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![
                    crate::selector::ClassSelector::new(None, "agent-smith"),
                    crate::selector::ClassSelector::new(None, "agent-brown"),
                    crate::selector::ClassSelector::new(None, "smithy"),
                ],
                default_agent: None,
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        let filtered = state.filtered_agents();
        let keys: Vec<String> = filtered
            .iter()
            .map(crate::selector::ClassSelector::key)
            .collect();

        assert_eq!(
            filtered.len(),
            2,
            "query 'smith' should match exactly 2 of 3 allowed agents"
        );
        assert!(keys.contains(&"agent-smith".to_string()));
        assert!(keys.contains(&"smithy".to_string()));
        assert!(!keys.contains(&"agent-brown".to_string()));
    }
}
