//! Pure console product rules.

use std::collections::{HashMap, HashSet};

use crate::app::context::eligible_roles_for_workspace;
use crate::agent::Agent;
use crate::config::{
    AppConfig, AuthForwardMode, GithubAuthMode, MountEntry, RoleSource, WorkspaceRoleOverride,
};
use crate::selector::RoleSelector;
use crate::workspace::{
    LoadWorkspaceInput, MountConfig, ResolvedWorkspace, WorkspaceConfig, WorkspaceEdit,
    current_dir_workspace,
};
use jackin_console::tui::auth::{AuthKind, AuthMode};

impl jackin_console::github_mounts::WorkspaceMounts for WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|mount| mount.src.as_str())
    }
}

#[must_use]
pub const fn auth_kind_agent(kind: AuthKind) -> Option<Agent> {
    match kind {
        AuthKind::Claude => Some(Agent::Claude),
        AuthKind::Codex => Some(Agent::Codex),
        AuthKind::Amp => Some(Agent::Amp),
        AuthKind::Kimi => Some(Agent::Kimi),
        AuthKind::Opencode => Some(Agent::Opencode),
        AuthKind::Github | AuthKind::Zai => None,
    }
}

#[must_use]
pub fn role_override_present(kind: AuthKind, ro: &WorkspaceRoleOverride) -> bool {
    match kind {
        AuthKind::Claude => ro.claude.is_some(),
        AuthKind::Codex => ro.codex.is_some(),
        AuthKind::Amp => ro.amp.is_some(),
        AuthKind::Kimi => ro.kimi.is_some(),
        AuthKind::Opencode => ro.opencode.is_some(),
        AuthKind::Github => ro.github.is_some(),
        AuthKind::Zai => ro.env.contains_key(crate::env_model::ZAI_API_KEY_ENV_NAME),
    }
}

#[must_use]
pub const fn auth_mode_to_auth_forward(mode: AuthMode) -> Option<AuthForwardMode> {
    match mode {
        AuthMode::Sync => Some(AuthForwardMode::Sync),
        AuthMode::ApiKey => Some(AuthForwardMode::ApiKey),
        AuthMode::OAuthToken => Some(AuthForwardMode::OAuthToken),
        AuthMode::Ignore => Some(AuthForwardMode::Ignore),
        AuthMode::Token => None,
    }
}

#[must_use]
pub const fn auth_mode_to_github(mode: AuthMode) -> Option<GithubAuthMode> {
    match mode {
        AuthMode::Sync => Some(GithubAuthMode::Sync),
        AuthMode::Token => Some(GithubAuthMode::Token),
        AuthMode::Ignore => Some(GithubAuthMode::Ignore),
        AuthMode::ApiKey | AuthMode::OAuthToken => None,
    }
}

#[must_use]
pub const fn auth_mode_from_auth_forward(mode: AuthForwardMode) -> AuthMode {
    match mode {
        AuthForwardMode::Sync => AuthMode::Sync,
        AuthForwardMode::ApiKey => AuthMode::ApiKey,
        AuthForwardMode::OAuthToken => AuthMode::OAuthToken,
        AuthForwardMode::Ignore => AuthMode::Ignore,
    }
}

#[must_use]
pub const fn auth_mode_from_github(mode: GithubAuthMode) -> AuthMode {
    match mode {
        GithubAuthMode::Sync => AuthMode::Sync,
        GithubAuthMode::Token => AuthMode::Token,
        GithubAuthMode::Ignore => AuthMode::Ignore,
    }
}

#[derive(Debug)]
pub(crate) struct InstanceRefreshSnapshot {
    pub(crate) instances: Vec<crate::instance::InstanceIndexEntry>,
    pub(crate) sessions: HashMap<String, Vec<crate::instance::SessionRecord>>,
    pub(crate) session_errors: HashSet<String>,
    pub(crate) snapshots: HashMap<String, crate::runtime::snapshot::InstanceSnapshot>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceChoice {
    pub name: String,
    pub workspace: ResolvedWorkspace,
    pub allowed_roles: Vec<RoleSelector>,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
    pub global_mounts: Vec<MountConfig>,
    pub input: LoadWorkspaceInput,
}

/// Resolve the role source the console should load for an operator-entered selector.
pub fn candidate_role_source(
    config: &AppConfig,
    selector: &RoleSelector,
) -> anyhow::Result<RoleSource> {
    let mut candidate = config.clone();
    match candidate.resolve_role_source(selector) {
        Ok((source, _)) => Ok(source),
        Err(_) if selector.namespace.is_none() => Ok(RoleSource {
            git: format!(
                "https://github.com/jackin-project/jackin-{}.git",
                selector.name
            ),
            trusted: false,
            env: std::collections::BTreeMap::new(),
        }),
        Err(err) => Err(err),
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
                    default_agent: None,
                    keep_awake_enabled: false,
                    git_pull_on_entry: false,
                },
                allowed_roles: configured_agents(config),
                default_role: None,
                last_role: None,
                global_mounts,
                input: LoadWorkspaceInput::CurrentDir,
            }))
        }
        LoadWorkspaceInput::Saved(name) => {
            let Some(saved) = config.workspaces.get(name) else {
                return Ok(None);
            };
            let allowed_roles = eligible_roles_for_workspace(config, saved);
            Ok(Some(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                    default_agent: saved.default_agent,
                    keep_awake_enabled: saved.keep_awake.enabled,
                    git_pull_on_entry: saved.git_pull_on_entry,
                },
                allowed_roles,
                default_role: saved.default_role.clone(),
                last_role: saved.last_role.clone(),
                global_mounts,
                input: LoadWorkspaceInput::Saved(name.clone()),
            }))
        }
        // CLI-only shape (`jackin load --path`); console never
        // produces it.
        LoadWorkspaceInput::Path { .. } => Ok(None),
    }
}

fn zai_key_present(config: &AppConfig, workspace_name: &str, role_selector: &str) -> bool {
    crate::operator_env::lookup_operator_env_raw(
        config,
        Some(role_selector),
        Some(workspace_name),
        "ZAI_API_KEY",
    )
    .is_some()
}

pub(in crate::console) fn providers_for_launch(
    config: &AppConfig,
    workspace_name: &str,
    role_selector: &str,
    agent: crate::agent::Agent,
) -> Vec<jackin_protocol::Provider> {
    jackin_protocol::Provider::available_for(
        agent.slug(),
        zai_key_present(config, workspace_name, role_selector),
    )
}

fn configured_agents(config: &AppConfig) -> Vec<RoleSelector> {
    config
        .roles
        .keys()
        .filter_map(|key| RoleSelector::parse(key).ok())
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

/// Build the config-editor patch for a workspace edit from original/pending UI state.
pub(crate) fn build_workspace_edit(
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) -> WorkspaceEdit {
    let mut edit = WorkspaceEdit::default();
    if pending.workdir != original.workdir {
        edit.workdir = Some(pending.workdir.clone());
    }
    for m in &pending.mounts {
        if !original.mounts.iter().any(|o| o == m) {
            edit.upsert_mounts.push(m.clone());
        }
    }
    for o in &original.mounts {
        if !pending.mounts.iter().any(|p| p.dst == o.dst) {
            edit.remove_destinations.push(o.dst.clone());
        }
    }
    for a in &pending.allowed_roles {
        if !original.allowed_roles.contains(a) {
            edit.allowed_roles_to_add.push(a.clone());
        }
    }
    for a in &original.allowed_roles {
        if !pending.allowed_roles.contains(a) {
            edit.allowed_roles_to_remove.push(a.clone());
        }
    }
    if pending.default_role != original.default_role {
        edit.default_role = Some(pending.default_role.clone());
    }
    if pending.keep_awake.enabled != original.keep_awake.enabled {
        edit.keep_awake_enabled = Some(pending.keep_awake.enabled);
    }
    if pending.git_pull_on_entry != original.git_pull_on_entry {
        edit.git_pull_on_entry_enabled = Some(pending.git_pull_on_entry);
    }
    edit
}

pub(crate) fn global_rows_have_sensitive_mount(rows: &[crate::config::GlobalMountRow]) -> bool {
    let mounts = rows
        .iter()
        .map(|row| row.mount.clone())
        .collect::<Vec<MountConfig>>();
    !crate::workspace::find_sensitive_mounts(&mounts).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthForwardMode, GithubAuthMode, WorkspaceRoleOverride};
    use jackin_console::tui::auth::AuthKind;

    #[test]
    fn auth_kind_agent_returns_none_for_github() {
        assert_eq!(auth_kind_agent(AuthKind::Github), None);
        assert_eq!(auth_kind_agent(AuthKind::Claude), Some(Agent::Claude));
        assert_eq!(auth_kind_agent(AuthKind::Codex), Some(Agent::Codex));
        assert_eq!(auth_kind_agent(AuthKind::Amp), Some(Agent::Amp));
        assert_eq!(auth_kind_agent(AuthKind::Kimi), Some(Agent::Kimi));
        assert_eq!(auth_kind_agent(AuthKind::Opencode), Some(Agent::Opencode));
    }

    #[test]
    fn auth_mode_to_auth_forward_round_trip() {
        for mode in [
            AuthForwardMode::Sync,
            AuthForwardMode::ApiKey,
            AuthForwardMode::OAuthToken,
            AuthForwardMode::Ignore,
        ] {
            assert_eq!(
                auth_mode_to_auth_forward(auth_mode_from_auth_forward(mode)),
                Some(mode)
            );
        }
    }

    #[test]
    fn auth_mode_to_github_round_trip() {
        for mode in [
            GithubAuthMode::Sync,
            GithubAuthMode::Token,
            GithubAuthMode::Ignore,
        ] {
            assert_eq!(
                auth_mode_to_github(auth_mode_from_github(mode)),
                Some(mode)
            );
        }
    }

    #[test]
    fn role_override_present_false_when_no_blocks_set() {
        let ro = WorkspaceRoleOverride::default();
        assert!(!role_override_present(AuthKind::Claude, &ro));
        assert!(!role_override_present(AuthKind::Codex, &ro));
        assert!(!role_override_present(AuthKind::Amp, &ro));
        assert!(!role_override_present(AuthKind::Kimi, &ro));
        assert!(!role_override_present(AuthKind::Opencode, &ro));
        assert!(!role_override_present(AuthKind::Github, &ro));
        assert!(!role_override_present(AuthKind::Zai, &ro));
    }

    #[test]
    fn role_override_present_zai_keys_off_env_var() {
        let mut ro = WorkspaceRoleOverride::default();
        assert!(!role_override_present(AuthKind::Zai, &ro));
        ro.env.insert(
            crate::env_model::ZAI_API_KEY_ENV_NAME.to_string(),
            crate::operator_env::EnvValue::Plain("k".into()),
        );
        assert!(role_override_present(AuthKind::Zai, &ro));
        assert!(!role_override_present(AuthKind::Claude, &ro));
        assert!(!role_override_present(AuthKind::Github, &ro));
    }

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
        config.roles.insert(
            "agent-smith".to_string(),
            crate::config::RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "ws".to_string(),
            crate::workspace::WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: workdir.clone(),
                mounts: vec![crate::workspace::MountConfig {
                    src: workdir.clone(),
                    dst: workdir,
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_roles: vec!["agent-smith".to_string()],
                default_role: Some("agent-smith".to_string()),
                default_agent: None,
                last_role: None,
                env: std::collections::BTreeMap::new(),
                roles: std::collections::BTreeMap::new(),
                keep_awake: crate::workspace::KeepAwakeConfig::default(),
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
                git_pull_on_entry: false,
            },
        );

        let choice = build_workspace_choice(
            &config,
            &project_dir,
            &LoadWorkspaceInput::Saved("ws".into()),
        )
        .unwrap()
        .expect("present saved workspace must resolve");
        assert_eq!(choice.default_role.as_deref(), Some("agent-smith"));
        assert_eq!(choice.allowed_roles.len(), 1);
    }

    // ── role-eligibility composition ───────────────────────────────

    fn agent_source_stub() -> crate::config::RoleSource {
        crate::config::RoleSource {
            git: "https://example.invalid/org/repo.git".to_string(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        }
    }

    fn workspace_with_allowed(allowed: &[&str]) -> crate::workspace::WorkspaceConfig {
        crate::workspace::WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/work".to_string(),
            mounts: vec![],
            allowed_roles: allowed.iter().map(|s| (*s).to_string()).collect(),
            default_role: None,
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            git_pull_on_entry: false,
        }
    }

    #[test]
    fn eligible_agents_returns_all_configured_when_allowed_list_empty() {
        let mut config = crate::config::AppConfig::default();
        config
            .roles
            .insert("alice".to_string(), agent_source_stub());
        config.roles.insert("bob".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&[]);
        let eligible = eligible_roles_for_workspace(&config, &ws);
        let keys: Vec<String> = eligible
            .iter()
            .map(crate::selector::RoleSelector::key)
            .collect();

        assert_eq!(eligible.len(), 2, "empty allowed_roles must mean 'any'");
        assert!(keys.contains(&"alice".to_string()));
        assert!(keys.contains(&"bob".to_string()));
    }

    #[test]
    fn eligible_agents_narrows_to_allowed_list_when_non_empty() {
        let mut config = crate::config::AppConfig::default();
        config
            .roles
            .insert("alice".to_string(), agent_source_stub());
        config.roles.insert("bob".to_string(), agent_source_stub());
        config
            .roles
            .insert("carol".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&["alice", "carol"]);
        let eligible = eligible_roles_for_workspace(&config, &ws);
        let keys: Vec<String> = eligible
            .iter()
            .map(crate::selector::RoleSelector::key)
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
            .roles
            .insert("alice".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&["ghost"]);
        let eligible = eligible_roles_for_workspace(&config, &ws);

        assert!(
            eligible.is_empty(),
            "eligibility must not resurrect a name absent from config.roles"
        );
    }
}
