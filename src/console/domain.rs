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

/// Resolve the effective auth mode for the panel via the kind-specific
/// resolver in `crate::config`. Agent kinds go through `resolve_mode`;
/// Github routes through `resolve_github_mode`.
#[must_use]
pub(crate) fn resolve_panel_mode(
    cfg: &AppConfig,
    kind: AuthKind,
    workspace: &str,
    role: &str,
) -> AuthMode {
    match kind {
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode => {
            let Some(agent) = auth_kind_agent(kind) else {
                return AuthMode::Ignore;
            };
            let mode = crate::config::resolve_mode(cfg, agent, workspace, role);
            auth_mode_from_auth_forward(mode)
        }
        AuthKind::Github => {
            let mode = crate::config::resolve_github_mode(cfg, workspace, role);
            auth_mode_from_github(mode)
        }
        AuthKind::Zai => {
            let key_present = crate::operator_env::lookup_operator_env_raw(
                cfg,
                (!role.is_empty()).then_some(role),
                Some(workspace),
                crate::env_model::ZAI_API_KEY_ENV_NAME,
            )
            .is_some();
            if key_present {
                AuthMode::ApiKey
            } else {
                AuthMode::Ignore
            }
        }
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

pub(crate) struct ResolvedRoleInput {
    pub(crate) raw: String,
    pub(crate) key: String,
    pub(crate) selector: RoleSelector,
    pub(crate) source: RoleSource,
}

pub(crate) struct RoleInputResolutionError {
    pub(crate) raw: String,
    pub(crate) source_url: Option<String>,
    pub(crate) error: anyhow::Error,
}

pub(crate) fn resolve_role_input_source(
    config: &AppConfig,
    value: &str,
) -> Result<ResolvedRoleInput, RoleInputResolutionError> {
    let raw = value.trim();
    crate::debug_log!("role", "resolving role loader input: raw={raw:?}");
    let selector = RoleSelector::parse(raw).map_err(|e| {
        crate::debug_log!("role", "role selector parse failed for {raw:?}: {e}");
        RoleInputResolutionError {
            raw: raw.to_string(),
            source_url: None,
            error: anyhow::Error::new(e),
        }
    })?;
    crate::debug_log!("role", "parsed role selector: {selector}");

    let key = selector.key();
    let source = candidate_role_source(config, &selector).map_err(|error| {
        crate::debug_log!(
            "role",
            "role loader failed for key={key:?} raw={raw:?}: {error:?}"
        );
        let source_url = candidate_role_source(config, &selector)
            .ok()
            .map(|source| source.git);
        RoleInputResolutionError {
            raw: raw.to_string(),
            source_url,
            error,
        }
    })?;
    crate::debug_log!(
        "role",
        "resolved candidate role source: key={key:?} git={git:?} trusted={trusted}",
        git = source.git.as_str(),
        trusted = source.trusted
    );
    Ok(ResolvedRoleInput {
        raw: raw.to_string(),
        key,
        selector,
        source,
    })
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

#[derive(Debug)]
pub(crate) enum LaunchDispatchResolution {
    NoEligibleRoles {
        name: String,
    },
    SingleRole {
        role: RoleSelector,
        workspace: ResolvedWorkspace,
    },
    RolePicker {
        input: LoadWorkspaceInput,
        roles: Vec<RoleSelector>,
        selected: Option<usize>,
    },
}

pub(crate) fn resolve_launch_dispatch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<LaunchDispatchResolution>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let roles = choice.allowed_roles.clone();

    if roles.is_empty() {
        return Ok(Some(LaunchDispatchResolution::NoEligibleRoles {
            name: choice.name,
        }));
    }

    if roles.len() == 1 {
        let role = roles.into_iter().next().unwrap();
        let workspace =
            crate::console::preview::resolve_selected_workspace(config, cwd, &choice, &role)?;
        return Ok(Some(LaunchDispatchResolution::SingleRole { role, workspace }));
    }

    let selected = crate::app::context::preferred_agent_index(
        &roles,
        choice.last_role.as_deref(),
        choice.default_role.as_deref(),
    );
    Ok(Some(LaunchDispatchResolution::RolePicker {
        input,
        roles,
        selected,
    }))
}

pub(crate) struct CommittedRoleLaunch {
    pub(crate) input: LoadWorkspaceInput,
    pub(crate) workspace: ResolvedWorkspace,
}

pub(crate) fn resolve_committed_role_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
    role: &RoleSelector,
) -> anyhow::Result<Option<CommittedRoleLaunch>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = crate::console::preview::resolve_selected_workspace(config, cwd, &choice, role)?;
    Ok(Some(CommittedRoleLaunch { input, workspace }))
}

pub(crate) struct CommittedAgentLaunch {
    pub(crate) input: LoadWorkspaceInput,
    pub(crate) role: RoleSelector,
    pub(crate) workspace: ResolvedWorkspace,
    pub(crate) providers: Vec<jackin_protocol::Provider>,
}

pub(crate) fn resolve_committed_agent_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
    role: RoleSelector,
    agent: Agent,
) -> anyhow::Result<Option<CommittedAgentLaunch>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace =
        crate::console::preview::resolve_selected_workspace(config, cwd, &choice, &role)?;
    let providers = providers_for_launch(config, &choice.name, &role.key(), agent);
    Ok(Some(CommittedAgentLaunch {
        input,
        role,
        workspace,
        providers,
    }))
}

pub(crate) fn resolve_provider_launch_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: &LoadWorkspaceInput,
    selector: &RoleSelector,
) -> anyhow::Result<Option<ResolvedWorkspace>> {
    let Some(choice) = build_workspace_choice(config, cwd, input)? else {
        return Ok(None);
    };
    crate::console::preview::resolve_selected_workspace(config, cwd, &choice, selector).map(Some)
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

pub(crate) enum EditorSavePreviewInput<'a> {
    Edit {
        original_name: &'a str,
        original: &'a WorkspaceConfig,
        pending: &'a WorkspaceConfig,
    },
    Create {
        pending: &'a WorkspaceConfig,
        pending_name: Option<&'a str>,
    },
}

pub(crate) enum EditorSavePreviewPlan {
    Edit {
        effective_removals: Vec<String>,
        edit_driven_collapses: Vec<crate::workspace::Removal>,
    },
    Create {
        final_mounts: Vec<MountConfig>,
        collapsed: Vec<crate::workspace::Removal>,
    },
}

pub(crate) enum EditorSavePreviewError {
    Message(String),
    PreExistingRedundantMounts {
        original_name: String,
        collapses: Vec<crate::workspace::Removal>,
    },
}

pub(crate) fn plan_editor_save_preview(
    config: &AppConfig,
    input: EditorSavePreviewInput<'_>,
) -> Result<EditorSavePreviewPlan, EditorSavePreviewError> {
    match input {
        EditorSavePreviewInput::Edit {
            original_name,
            original,
            pending,
        } => {
            let current_ws = config.workspaces.get(original_name).cloned().ok_or_else(|| {
                EditorSavePreviewError::Message(format!(
                    "workspace {original_name:?} no longer exists in config"
                ))
            })?;
            let edit_delta = build_workspace_edit(original, pending);
            let plan = crate::workspace::planner::plan_edit(
                &current_ws,
                &edit_delta.upsert_mounts,
                &edit_delta.remove_destinations,
                false,
            )
            .map_err(|e| EditorSavePreviewError::Message(e.to_string()))?;
            if plan.edit_driven_collapses.is_empty() && !plan.pre_existing_collapses.is_empty() {
                return Err(EditorSavePreviewError::PreExistingRedundantMounts {
                    original_name: original_name.to_string(),
                    collapses: plan.pre_existing_collapses,
                });
            }
            Ok(EditorSavePreviewPlan::Edit {
                effective_removals: plan.effective_removals,
                edit_driven_collapses: plan.edit_driven_collapses,
            })
        }
        EditorSavePreviewInput::Create {
            pending,
            pending_name,
        } => {
            if pending_name.is_none() {
                return Err(EditorSavePreviewError::Message(
                    "missing workspace name".to_string(),
                ));
            }
            let plan = crate::workspace::planner::plan_create(&pending.mounts)
                .map_err(|e| EditorSavePreviewError::Message(e.to_string()))?;
            Ok(EditorSavePreviewPlan::Create {
                final_mounts: plan.final_mounts,
                collapsed: plan.collapsed,
            })
        }
    }
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

    fn launch_workspace(
        workdir: &std::path::Path,
        allowed_roles: Vec<&str>,
    ) -> crate::workspace::WorkspaceConfig {
        crate::workspace::WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: workdir.display().to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: workdir.display().to_string(),
                dst: workdir.display().to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: allowed_roles.into_iter().map(str::to_string).collect(),
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
    fn resolve_launch_dispatch_returns_none_for_deleted_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let config = crate::config::AppConfig::default();

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("missing".to_string()),
        )
        .unwrap();

        assert!(resolution.is_none());
    }

    #[test]
    fn resolve_launch_dispatch_reports_no_eligible_roles() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = crate::config::AppConfig::default();
        config.workspaces.insert(
            "empty".to_string(),
            launch_workspace(temp.path(), Vec::new()),
        );

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("empty".to_string()),
        )
        .unwrap()
        .expect("workspace exists");

        assert!(matches!(
            resolution,
            LaunchDispatchResolution::NoEligibleRoles { name } if name == "empty"
        ));
    }

    #[test]
    fn resolve_launch_dispatch_resolves_single_role_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = crate::config::AppConfig::default();
        config
            .roles
            .insert("smith".to_string(), agent_source_stub());
        config.workspaces.insert(
            "solo".to_string(),
            launch_workspace(temp.path(), vec!["smith"]),
        );

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("solo".to_string()),
        )
        .unwrap()
        .expect("workspace exists");

        let LaunchDispatchResolution::SingleRole { role, workspace } = resolution else {
            panic!("expected single-role launch dispatch");
        };
        assert_eq!(role.key(), "smith");
        assert_eq!(workspace.label, "solo");
    }

    #[test]
    fn resolve_launch_dispatch_preselects_role_picker() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = crate::config::AppConfig::default();
        config
            .roles
            .insert("alpha".to_string(), agent_source_stub());
        config.roles.insert("beta".to_string(), agent_source_stub());
        let mut saved = launch_workspace(temp.path(), vec!["alpha", "beta"]);
        saved.last_role = Some("beta".to_string());
        config.workspaces.insert("multi".to_string(), saved);

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("multi".to_string()),
        )
        .unwrap()
        .expect("workspace exists");

        let LaunchDispatchResolution::RolePicker {
            roles, selected, ..
        } = resolution
        else {
            panic!("expected role picker dispatch");
        };
        assert_eq!(
            roles
                .iter()
                .map(crate::selector::RoleSelector::key)
                .collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
        assert_eq!(selected, Some(1));
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
