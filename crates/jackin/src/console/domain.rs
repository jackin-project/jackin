//! Pure console product rules.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::agent::Agent;
use crate::app::context::eligible_roles_for_workspace;
use crate::config::{AppConfig, AuthForwardMode, MountEntry, RoleSource, WorkspaceRoleOverride};
use crate::isolation::MountIsolation;
use crate::selector::RoleSelector;
use crate::workspace::{
    LoadWorkspaceInput, MountConfig, ResolvedWorkspace, WorkspaceConfig, WorkspaceEdit,
    current_dir_workspace,
};
use jackin_console::tui::auth::{AuthKind, AuthMode};
use jackin_console::tui::auth_config::{
    auth_kind_agent, auth_mode_from_auth_forward, auth_mode_from_github,
};

// WorkspaceMounts impl for WorkspaceConfig now lives in jackin-console (orphan rule).

// Validate a picked source folder against the agent an auth form targets.
// Returns `Ok(())` for non-agent auth kinds. Runtime validation stays
// in the binary adapter because `jackin-console` cannot depend on runtime.
pub(in crate::console) fn validate_auth_source_folder(
    kind: Option<AuthKind>,
    path: &std::path::Path,
) -> Result<(), String> {
    let Some(agent) = kind.and_then(auth_kind_agent) else {
        return Ok(());
    };
    let host_home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    jackin_runtime::instance::validate_sync_source_dir(agent, path, &host_home)
}

pub(super) fn panel_mode_requires_credential(
    cfg: &AppConfig,
    workspace: &str,
    role: &str,
    kind: AuthKind,
) -> bool {
    let mode = resolve_panel_mode(cfg, kind, workspace, role);
    kind.required_env_var(mode).is_some()
}

/// Roles already carrying an override stay eligible: operators may add
/// more keys to an existing override.
#[must_use]
pub(super) fn eligible_role_keys_for_override(
    cfg: &AppConfig,
    workspace: &WorkspaceConfig,
) -> Vec<String> {
    if workspace.allowed_roles.is_empty() {
        cfg.roles.keys().cloned().collect()
    } else {
        workspace.allowed_roles.clone()
    }
}

#[must_use]
pub(super) fn settings_auth_env_value<'a>(
    kind: AuthKind,
    mode: AuthMode,
    github_env: &'a BTreeMap<String, crate::operator_env::EnvValue>,
    agent_env: &'a BTreeMap<String, crate::operator_env::EnvValue>,
) -> Option<&'a crate::operator_env::EnvValue> {
    let env_name = kind.required_env_var(mode)?;
    if kind == AuthKind::Github {
        github_env.get(env_name)
    } else {
        agent_env.get(env_name)
    }
}

#[must_use]
pub(super) fn workspace_auth_mode_and_credential(
    workspace: &WorkspaceConfig,
    kind: AuthKind,
) -> (Option<AuthMode>, Option<crate::operator_env::EnvValue>) {
    match kind {
        AuthKind::Claude => agent_workspace_mode_and_credential(
            workspace.claude.as_ref().map(|c| c.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Codex => agent_workspace_mode_and_credential(
            workspace.codex.as_ref().map(|c| c.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Amp => agent_workspace_mode_and_credential(
            workspace.amp.as_ref().map(|c| c.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Kimi => agent_workspace_mode_and_credential(
            workspace.kimi.as_ref().map(|c| c.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Opencode => agent_workspace_mode_and_credential(
            workspace.opencode.as_ref().map(|c| c.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Grok => agent_workspace_mode_and_credential(
            workspace.grok.as_ref().map(|c| c.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Github => {
            let mode = workspace
                .github
                .as_ref()
                .map(|github| auth_mode_from_github(github.auth_forward));
            let credential = mode
                .and_then(|mode| kind.required_env_var(mode))
                .and_then(|name| {
                    workspace
                        .github
                        .as_ref()
                        .and_then(|github| github.env.get(name).cloned())
                });
            (mode, credential)
        }
        AuthKind::Zai => {
            env_only_mode_and_credential(&workspace.env, crate::env_model::ZAI_API_KEY_ENV_NAME)
        }
        AuthKind::Minimax => {
            env_only_mode_and_credential(&workspace.env, crate::env_model::MINIMAX_API_KEY_ENV_NAME)
        }
    }
}

#[must_use]
pub(super) fn explicit_workspace_auth_mode(
    workspace: &WorkspaceConfig,
    kind: AuthKind,
) -> Option<AuthMode> {
    workspace_auth_mode_and_credential(workspace, kind).0
}

#[must_use]
pub(super) fn panel_auth_source_value<'a>(
    cfg: &'a AppConfig,
    workspace: &str,
    role: &str,
    env_name: &str,
    kind: AuthKind,
) -> Option<&'a crate::operator_env::EnvValue> {
    match kind {
        AuthKind::Github => github_panel_source_value(cfg, workspace, role, env_name),
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode
        | AuthKind::Grok
        | AuthKind::Zai
        | AuthKind::Minimax => agent_panel_source_value(cfg, workspace, role, env_name),
    }
}

fn agent_panel_source_value<'a>(
    cfg: &'a AppConfig,
    workspace: &str,
    role: &str,
    env_name: &str,
) -> Option<&'a crate::operator_env::EnvValue> {
    if !role.is_empty()
        && let Some(value) = cfg
            .workspaces
            .get(workspace)
            .and_then(|workspace| workspace.roles.get(role))
            .and_then(|role| role.env.get(env_name))
    {
        return Some(value);
    }
    if let Some(value) = cfg
        .workspaces
        .get(workspace)
        .and_then(|workspace| workspace.env.get(env_name))
    {
        return Some(value);
    }
    if !role.is_empty()
        && let Some(value) = cfg.roles.get(role).and_then(|role| role.env.get(env_name))
    {
        return Some(value);
    }
    cfg.env.get(env_name)
}

fn github_panel_source_value<'a>(
    cfg: &'a AppConfig,
    workspace: &str,
    role: &str,
    env_name: &str,
) -> Option<&'a crate::operator_env::EnvValue> {
    if !role.is_empty()
        && let Some(value) = cfg
            .workspaces
            .get(workspace)
            .and_then(|workspace| workspace.roles.get(role))
            .and_then(|role| role.github.as_ref())
            .and_then(|github| github.env.get(env_name))
    {
        return Some(value);
    }
    if let Some(value) = cfg
        .workspaces
        .get(workspace)
        .and_then(|workspace| workspace.github.as_ref())
        .and_then(|github| github.env.get(env_name))
    {
        return Some(value);
    }
    cfg.github
        .as_ref()
        .and_then(|github| github.env.get(env_name))
}

#[must_use]
pub(super) fn role_auth_mode_and_credential(
    role: Option<&WorkspaceRoleOverride>,
    kind: AuthKind,
) -> (Option<AuthMode>, Option<crate::operator_env::EnvValue>) {
    match kind {
        AuthKind::Claude => agent_role_mode_and_credential(
            role.and_then(|role| role.claude.as_ref())
                .map(|config| config.auth_forward),
            role,
            kind,
        ),
        AuthKind::Codex => agent_role_mode_and_credential(
            role.and_then(|role| role.codex.as_ref())
                .map(|config| config.auth_forward),
            role,
            kind,
        ),
        AuthKind::Amp => agent_role_mode_and_credential(
            role.and_then(|role| role.amp.as_ref())
                .map(|config| config.auth_forward),
            role,
            kind,
        ),
        AuthKind::Kimi => agent_role_mode_and_credential(
            role.and_then(|role| role.kimi.as_ref())
                .map(|config| config.auth_forward),
            role,
            kind,
        ),
        AuthKind::Opencode => agent_role_mode_and_credential(
            role.and_then(|role| role.opencode.as_ref())
                .map(|config| config.auth_forward),
            role,
            kind,
        ),
        AuthKind::Grok => agent_role_mode_and_credential(
            role.and_then(|role| role.grok.as_ref())
                .map(|config| config.auth_forward),
            role,
            kind,
        ),
        AuthKind::Github => {
            let mode = role
                .and_then(|role| role.github.as_ref())
                .map(|github| auth_mode_from_github(github.auth_forward));
            let credential = mode
                .and_then(|mode| kind.required_env_var(mode))
                .and_then(|name| {
                    role.and_then(|role| role.github.as_ref())
                        .and_then(|github| github.env.get(name).cloned())
                });
            (mode, credential)
        }
        AuthKind::Zai => role.map_or((None, None), |role| {
            env_only_mode_and_credential(&role.env, crate::env_model::ZAI_API_KEY_ENV_NAME)
        }),
        AuthKind::Minimax => role.map_or((None, None), |role| {
            env_only_mode_and_credential(&role.env, crate::env_model::MINIMAX_API_KEY_ENV_NAME)
        }),
    }
}

fn agent_workspace_mode_and_credential(
    auth_forward: Option<AuthForwardMode>,
    env: &BTreeMap<String, crate::operator_env::EnvValue>,
    kind: AuthKind,
) -> (Option<AuthMode>, Option<crate::operator_env::EnvValue>) {
    let mode = auth_forward.map(auth_mode_from_auth_forward);
    let credential = mode
        .and_then(|mode| kind.required_env_var(mode))
        .and_then(|name| env.get(name).cloned());
    (mode, credential)
}

fn agent_role_mode_and_credential(
    auth_forward: Option<AuthForwardMode>,
    role: Option<&WorkspaceRoleOverride>,
    kind: AuthKind,
) -> (Option<AuthMode>, Option<crate::operator_env::EnvValue>) {
    let mode = auth_forward.map(auth_mode_from_auth_forward);
    let credential = mode
        .and_then(|mode| kind.required_env_var(mode))
        .and_then(|name| role.and_then(|role| role.env.get(name).cloned()));
    (mode, credential)
}

fn env_only_mode_and_credential(
    env: &BTreeMap<String, crate::operator_env::EnvValue>,
    key: &str,
) -> (Option<AuthMode>, Option<crate::operator_env::EnvValue>) {
    let credential = env.get(key).cloned();
    let mode = credential.as_ref().map(|_| AuthMode::ApiKey);
    (mode, credential)
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
        | AuthKind::Opencode
        | AuthKind::Grok => {
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
        AuthKind::Zai | AuthKind::Minimax => {
            // Env-only provider kinds: mode derived from whether the key is present
            // in the effective env at this layer. Fails loudly if the AuthKind has no
            // ApiKey env mapping, which is a programming error.
            let env_key = kind
                .required_env_var(AuthMode::ApiKey)
                .expect("env-only provider AuthKind must define an ApiKey env var");
            let key_present = crate::operator_env::lookup_operator_env_raw(
                cfg,
                (!role.is_empty()).then_some(role),
                Some(workspace),
                env_key,
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
pub(super) fn candidate_role_source(
    config: &AppConfig,
    selector: &RoleSelector,
) -> anyhow::Result<RoleSource> {
    let mut candidate = config.clone();
    match candidate.resolve_role_source(selector) {
        Ok((source, _)) => Ok(source),
        Err(_) if selector.namespace.is_none() => Ok(RoleSource {
            // Per project convention, agent roles on GitHub are always
            // named with the `jackin-` prefix.
            git: format!(
                "https://github.com/jackin-project/jackin-{}.git",
                selector.name
            ),
            trusted: false,
            env: BTreeMap::new(),
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
            raw: raw.to_owned(),
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
            raw: raw.to_owned(),
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
        raw: raw.to_owned(),
        key,
        selector,
        source,
    })
}

#[must_use]
pub(crate) fn current_dir_mount_config(cwd_str: &str) -> MountConfig {
    shared_mount_config(cwd_str, cwd_str, false)
}

#[must_use]
pub(crate) fn shared_mount_config(
    src: impl Into<String>,
    dst: impl Into<String>,
    readonly: bool,
) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly,
        isolation: MountIsolation::Shared,
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
                name: "Current directory".to_owned(),
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
        let workspace = resolve_selected_workspace(config, cwd, &choice, &role)?;
        return Ok(Some(LaunchDispatchResolution::SingleRole {
            role,
            workspace,
        }));
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
    let workspace = resolve_selected_workspace(config, cwd, &choice, role)?;
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
    let workspace = resolve_selected_workspace(config, cwd, &choice, &role)?;
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
    resolve_selected_workspace(config, cwd, &choice, selector).map(Some)
}

fn resolve_selected_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    role: &RoleSelector,
) -> anyhow::Result<ResolvedWorkspace> {
    crate::workspace::resolve_load_workspace(config, role, cwd, choice.input.clone(), &[])
}

fn operator_key_present(
    config: &AppConfig,
    workspace_name: &str,
    role_selector: &str,
    env_var: &str,
) -> bool {
    crate::operator_env::lookup_operator_env_raw(
        config,
        Some(role_selector),
        Some(workspace_name),
        env_var,
    )
    .is_some()
}

pub(in crate::console) fn providers_for_launch(
    config: &AppConfig,
    workspace_name: &str,
    role_selector: &str,
    agent: Agent,
) -> Vec<jackin_protocol::Provider> {
    // Map each provider to whether the operator configured its key, using the
    // same `key_env_var()` accessor the capsule reads so the two surfaces cannot
    // disagree. `available_for` only consults this for agents that actually need
    // a key (`needs_key_for_agent`), so Anthropic+claude still passes on
    // subscription auth while Anthropic+opencode requires `ANTHROPIC_API_KEY`.
    // `is_none_or` passes any provider that has no key variable at all.
    let key = |env_var: &str| operator_key_present(config, workspace_name, role_selector, env_var);
    jackin_protocol::Provider::available_for(agent.slug(), |provider: jackin_protocol::Provider| {
        provider.key_env_var().is_none_or(&key)
    })
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

#[must_use]
pub(crate) fn pre_existing_redundant_mounts_message(
    original_name: &str,
    collapses: &[crate::workspace::Removal],
) -> String {
    let details: Vec<String> = collapses
        .iter()
        .map(|r| {
            format!(
                "{} covered by {}",
                crate::tui::shorten_home(&r.child.src),
                crate::tui::shorten_home(&r.covered_by.src),
            )
        })
        .collect();
    format!(
        "pre-existing redundant mount(s) in this workspace: {}; \
         run `jackin' workspace prune {original_name}` to clean up",
        details.join(", "),
    )
}

/// Mirror the merge order `AppConfig::edit_workspace` uses to build the
/// post-edit mount list, so the source-drift check evaluates the same
/// shape that will land on disk.
#[must_use]
pub(crate) fn prospective_workspace_mounts(
    current: &[MountConfig],
    pending: &[MountConfig],
    effective_removals: &[String],
) -> Vec<MountConfig> {
    let mut out: Vec<MountConfig> = current
        .iter()
        .filter(|m| !effective_removals.iter().any(|d| d == &m.dst))
        .cloned()
        .collect();
    for upsert in pending {
        if let Some(existing) = out.iter_mut().find(|existing| existing.dst == upsert.dst) {
            *existing = upsert.clone();
        } else {
            out.push(upsert.clone());
        }
    }
    out
}

#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
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
            let current_ws = config
                .workspaces
                .get(original_name)
                .cloned()
                .ok_or_else(|| {
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
                    original_name: original_name.to_owned(),
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
                    "missing workspace name".to_owned(),
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

#[must_use]
pub(crate) fn global_mount_scope_value(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

#[must_use]
pub(crate) fn unique_global_mount_name(
    rows: &[crate::config::GlobalMountRow],
    scope: Option<&str>,
    dst: &str,
) -> String {
    let basename = std::path::Path::new(dst)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("mount");
    let base = basename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    let base = if base.is_empty() {
        "mount".to_owned()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while rows
        .iter()
        .any(|row| row.scope.as_deref() == scope && row.name == candidate)
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

#[cfg(test)]
mod tests;
