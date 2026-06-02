//! Pure console product rules.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::app::context::eligible_roles_for_workspace;
use crate::agent::Agent;
use crate::config::{
    AgentAuthConfig, AmpAuthConfig, AppConfig, AuthForwardMode, CodexAuthConfig, GithubAuthConfig,
    GithubAuthMode, KimiAuthConfig, MountEntry, OpencodeAuthConfig, RoleSource,
    WorkspaceRoleOverride,
};
use crate::isolation::MountIsolation;
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
pub fn github_auth_config_with_preserved_env(
    mode: Option<AuthMode>,
    existing: Option<&GithubAuthConfig>,
) -> Option<GithubAuthConfig> {
    mode.and_then(auth_mode_to_github).map(|auth_forward| GithubAuthConfig {
        auth_forward,
        env: existing.map(|github| github.env.clone()).unwrap_or_default(),
    })
}

pub fn set_workspace_auth_mode(
    ws: &mut WorkspaceConfig,
    kind: AuthKind,
    mode: Option<AuthMode>,
) {
    set_auth_mode(ws, kind, mode);
}

pub fn set_role_auth_mode(
    role: &mut WorkspaceRoleOverride,
    kind: AuthKind,
    mode: Option<AuthMode>,
) {
    set_auth_mode(role, kind, mode);
}

trait AuthLayerMut {
    fn github_auth(&self) -> Option<&GithubAuthConfig>;
    fn set_claude_auth(&mut self, auth: Option<AgentAuthConfig>);
    fn set_codex_auth(&mut self, auth: Option<CodexAuthConfig>);
    fn set_amp_auth(&mut self, auth: Option<AmpAuthConfig>);
    fn set_kimi_auth(&mut self, auth: Option<KimiAuthConfig>);
    fn set_opencode_auth(&mut self, auth: Option<OpencodeAuthConfig>);
    fn set_github_auth(&mut self, auth: Option<GithubAuthConfig>);
}

impl AuthLayerMut for WorkspaceConfig {
    fn github_auth(&self) -> Option<&GithubAuthConfig> {
        self.github.as_ref()
    }

    fn set_claude_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.claude = auth;
    }

    fn set_codex_auth(&mut self, auth: Option<CodexAuthConfig>) {
        self.codex = auth;
    }

    fn set_amp_auth(&mut self, auth: Option<AmpAuthConfig>) {
        self.amp = auth;
    }

    fn set_kimi_auth(&mut self, auth: Option<KimiAuthConfig>) {
        self.kimi = auth;
    }

    fn set_opencode_auth(&mut self, auth: Option<OpencodeAuthConfig>) {
        self.opencode = auth;
    }

    fn set_github_auth(&mut self, auth: Option<GithubAuthConfig>) {
        self.github = auth;
    }
}

impl AuthLayerMut for WorkspaceRoleOverride {
    fn github_auth(&self) -> Option<&GithubAuthConfig> {
        self.github.as_ref()
    }

    fn set_claude_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.claude = auth;
    }

    fn set_codex_auth(&mut self, auth: Option<CodexAuthConfig>) {
        self.codex = auth;
    }

    fn set_amp_auth(&mut self, auth: Option<AmpAuthConfig>) {
        self.amp = auth;
    }

    fn set_kimi_auth(&mut self, auth: Option<KimiAuthConfig>) {
        self.kimi = auth;
    }

    fn set_opencode_auth(&mut self, auth: Option<OpencodeAuthConfig>) {
        self.opencode = auth;
    }

    fn set_github_auth(&mut self, auth: Option<GithubAuthConfig>) {
        self.github = auth;
    }
}

fn set_auth_mode(layer: &mut impl AuthLayerMut, kind: AuthKind, mode: Option<AuthMode>) {
    match kind {
        AuthKind::Claude => {
            layer.set_claude_auth(
                mode
                .and_then(auth_mode_to_auth_forward)
                    .map(|auth_forward| AgentAuthConfig { auth_forward }),
            );
        }
        AuthKind::Codex => {
            layer.set_codex_auth(
                mode
                .and_then(auth_mode_to_auth_forward)
                    .map(|auth_forward| CodexAuthConfig(AgentAuthConfig { auth_forward })),
            );
        }
        AuthKind::Amp => {
            layer.set_amp_auth(
                mode
                .and_then(auth_mode_to_auth_forward)
                    .map(|auth_forward| AmpAuthConfig(AgentAuthConfig { auth_forward })),
            );
        }
        AuthKind::Kimi => {
            layer.set_kimi_auth(
                mode
                .and_then(auth_mode_to_auth_forward)
                    .map(|auth_forward| KimiAuthConfig(AgentAuthConfig { auth_forward })),
            );
        }
        AuthKind::Opencode => {
            layer.set_opencode_auth(
                mode
                .and_then(auth_mode_to_auth_forward)
                    .map(|auth_forward| OpencodeAuthConfig(AgentAuthConfig { auth_forward })),
            );
        }
        AuthKind::Github => {
            layer.set_github_auth(github_auth_config_with_preserved_env(mode, layer.github_auth()));
        }
        AuthKind::Zai => {}
    }
}

pub fn apply_workspace_auth_commit(
    ws: &mut WorkspaceConfig,
    kind: AuthKind,
    mode: AuthMode,
    env_var_name: Option<&str>,
    env_value: Option<crate::operator_env::EnvValue>,
) {
    set_workspace_auth_mode(ws, kind, Some(mode));
    if kind == AuthKind::Zai && mode == AuthMode::Ignore {
        ws.env.remove(crate::env_model::ZAI_API_KEY_ENV_NAME);
    }
    apply_auth_env_value(&mut ws.env, ws.github.as_mut(), kind, env_var_name, env_value);
}

pub fn apply_role_auth_commit(
    role: &mut WorkspaceRoleOverride,
    kind: AuthKind,
    mode: AuthMode,
    env_var_name: Option<&str>,
    env_value: Option<crate::operator_env::EnvValue>,
) {
    set_role_auth_mode(role, kind, Some(mode));
    if kind == AuthKind::Zai && mode == AuthMode::Ignore {
        role.env.remove(crate::env_model::ZAI_API_KEY_ENV_NAME);
    }
    apply_auth_env_value(
        &mut role.env,
        role.github.as_mut(),
        kind,
        env_var_name,
        env_value,
    );
}

pub fn clear_workspace_auth_layer(ws: &mut WorkspaceConfig, kind: AuthKind) {
    set_workspace_auth_mode(ws, kind, None);
}

pub fn clear_role_auth_layer(role: &mut WorkspaceRoleOverride, kind: AuthKind) {
    set_role_auth_mode(role, kind, None);
}

pub fn apply_settings_auth_env_commit(
    kind: AuthKind,
    env_var_name: Option<&str>,
    env_value: Option<crate::operator_env::EnvValue>,
    github_env: &mut BTreeMap<String, crate::operator_env::EnvValue>,
    agent_env: &mut BTreeMap<String, crate::operator_env::EnvValue>,
) {
    let (Some(name), Some(value)) = (env_var_name, env_value) else {
        return;
    };
    settings_auth_env_map_mut(kind, github_env, agent_env).insert(name.to_string(), value);
}

pub fn clear_settings_auth_env_values(
    kind: AuthKind,
    github_env: &mut BTreeMap<String, crate::operator_env::EnvValue>,
    agent_env: &mut BTreeMap<String, crate::operator_env::EnvValue>,
) {
    for mode in kind.supported_modes() {
        if let Some(env_var) = kind.required_env_var(*mode) {
            settings_auth_env_map_mut(kind, github_env, agent_env).remove(env_var);
        }
    }
}

fn settings_auth_env_map_mut<'a>(
    kind: AuthKind,
    github_env: &'a mut BTreeMap<String, crate::operator_env::EnvValue>,
    agent_env: &'a mut BTreeMap<String, crate::operator_env::EnvValue>,
) -> &'a mut BTreeMap<String, crate::operator_env::EnvValue> {
    match kind {
        AuthKind::Github => github_env,
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode
        | AuthKind::Zai => agent_env,
    }
}

fn apply_auth_env_value(
    env: &mut BTreeMap<String, crate::operator_env::EnvValue>,
    github: Option<&mut GithubAuthConfig>,
    kind: AuthKind,
    env_var_name: Option<&str>,
    env_value: Option<crate::operator_env::EnvValue>,
) {
    let (Some(name), Some(value)) = (env_var_name, env_value) else {
        return;
    };
    match kind {
        AuthKind::Github => {
            if let Some(github) = github {
                github.env.insert(name.to_string(), value);
            }
        }
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode
        | AuthKind::Zai => {
            env.insert(name.to_string(), value);
        }
    }
}

#[must_use]
pub fn app_github_env(cfg: &AppConfig) -> BTreeMap<String, crate::operator_env::EnvValue> {
    cfg.github
        .as_ref()
        .map(|github| github.env.clone())
        .unwrap_or_default()
}

#[must_use]
pub fn panel_mode_requires_credential(
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
pub fn eligible_role_keys_for_override(cfg: &AppConfig, workspace: &WorkspaceConfig) -> Vec<String> {
    if workspace.allowed_roles.is_empty() {
        cfg.roles.keys().cloned().collect()
    } else {
        workspace.allowed_roles.clone()
    }
}

#[must_use]
pub fn settings_auth_env_value<'a>(
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
pub fn workspace_auth_mode_and_credential(
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
            workspace.codex.as_ref().map(|c| c.0.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Amp => agent_workspace_mode_and_credential(
            workspace.amp.as_ref().map(|c| c.0.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Kimi => agent_workspace_mode_and_credential(
            workspace.kimi.as_ref().map(|c| c.0.auth_forward),
            &workspace.env,
            kind,
        ),
        AuthKind::Opencode => agent_workspace_mode_and_credential(
            workspace.opencode.as_ref().map(|c| c.0.auth_forward),
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
        AuthKind::Zai => zai_mode_and_credential(&workspace.env),
    }
}

#[must_use]
pub fn explicit_workspace_auth_mode(
    workspace: &WorkspaceConfig,
    kind: AuthKind,
) -> Option<AuthMode> {
    workspace_auth_mode_and_credential(workspace, kind).0
}

#[must_use]
pub fn panel_auth_source_value<'a>(
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
        | AuthKind::Zai => agent_panel_source_value(cfg, workspace, role, env_name),
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
pub fn role_auth_mode_and_credential(
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
                .map(|config| config.0.auth_forward),
            role,
            kind,
        ),
        AuthKind::Amp => agent_role_mode_and_credential(
            role.and_then(|role| role.amp.as_ref())
                .map(|config| config.0.auth_forward),
            role,
            kind,
        ),
        AuthKind::Kimi => agent_role_mode_and_credential(
            role.and_then(|role| role.kimi.as_ref())
                .map(|config| config.0.auth_forward),
            role,
            kind,
        ),
        AuthKind::Opencode => agent_role_mode_and_credential(
            role.and_then(|role| role.opencode.as_ref())
                .map(|config| config.0.auth_forward),
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
        AuthKind::Zai => role.map_or((None, None), |role| zai_mode_and_credential(&role.env)),
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

fn zai_mode_and_credential(
    env: &BTreeMap<String, crate::operator_env::EnvValue>,
) -> (Option<AuthMode>, Option<crate::operator_env::EnvValue>) {
    let credential = env.get(crate::env_model::ZAI_API_KEY_ENV_NAME).cloned();
    let mode = credential.as_ref().map(|_| AuthMode::ApiKey);
    (mode, credential)
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

#[must_use]
pub(crate) fn global_mount_scope_value(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
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
        .to_string();
    let base = if base.is_empty() {
        "mount".to_string()
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
mod tests {
    use super::*;
    use crate::config::{AuthForwardMode, GithubAuthConfig, GithubAuthMode, WorkspaceRoleOverride};
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
    fn github_auth_config_preserves_env_on_mode_change() {
        let mut existing = GithubAuthConfig::default();
        existing.env.insert(
            "GH_TOKEN".to_string(),
            crate::operator_env::EnvValue::Plain("token".into()),
        );

        let next = github_auth_config_with_preserved_env(Some(AuthMode::Ignore), Some(&existing))
            .expect("github mode should build config");

        assert_eq!(next.auth_forward, GithubAuthMode::Ignore);
        assert_eq!(next.env, existing.env);
        assert!(github_auth_config_with_preserved_env(Some(AuthMode::ApiKey), Some(&existing)).is_none());
        assert!(github_auth_config_with_preserved_env(None, Some(&existing)).is_none());
    }

    #[test]
    fn apply_workspace_auth_commit_updates_mode_and_env_layer() {
        let mut ws = WorkspaceConfig::default();

        apply_workspace_auth_commit(
            &mut ws,
            AuthKind::Github,
            AuthMode::Token,
            Some("GH_TOKEN"),
            Some(crate::operator_env::EnvValue::Plain("token".into())),
        );

        let github = ws.github.expect("github auth should be stored");
        assert_eq!(github.auth_forward, GithubAuthMode::Token);
        assert_eq!(
            github.env.get("GH_TOKEN"),
            Some(&crate::operator_env::EnvValue::Plain("token".into()))
        );
        assert!(ws.env.is_empty());
    }

    #[test]
    fn apply_role_auth_commit_updates_mode_and_zai_ignore_removes_key() {
        let mut role = WorkspaceRoleOverride::default();
        role.env.insert(
            crate::env_model::ZAI_API_KEY_ENV_NAME.to_string(),
            crate::operator_env::EnvValue::Plain("stale".into()),
        );

        apply_role_auth_commit(&mut role, AuthKind::Zai, AuthMode::Ignore, None, None);

        assert!(!role.env.contains_key(crate::env_model::ZAI_API_KEY_ENV_NAME));
    }

    #[test]
    fn clear_workspace_auth_layer_removes_github_block() {
        let mut ws = WorkspaceConfig::default();
        apply_workspace_auth_commit(
            &mut ws,
            AuthKind::Github,
            AuthMode::Token,
            Some("GH_TOKEN"),
            Some(crate::operator_env::EnvValue::Plain("token".into())),
        );

        clear_workspace_auth_layer(&mut ws, AuthKind::Github);

        assert!(ws.github.is_none());
    }

    #[test]
    fn apply_settings_auth_env_commit_routes_by_kind() {
        let mut github_env = BTreeMap::new();
        let mut agent_env = BTreeMap::new();

        apply_settings_auth_env_commit(
            AuthKind::Github,
            Some("GH_TOKEN"),
            Some(crate::operator_env::EnvValue::Plain("token".into())),
            &mut github_env,
            &mut agent_env,
        );
        apply_settings_auth_env_commit(
            AuthKind::Claude,
            Some("ANTHROPIC_API_KEY"),
            Some(crate::operator_env::EnvValue::Plain("key".into())),
            &mut github_env,
            &mut agent_env,
        );

        assert_eq!(
            github_env.get("GH_TOKEN"),
            Some(&crate::operator_env::EnvValue::Plain("token".into()))
        );
        assert_eq!(
            agent_env.get("ANTHROPIC_API_KEY"),
            Some(&crate::operator_env::EnvValue::Plain("key".into()))
        );
    }

    #[test]
    fn clear_settings_auth_env_values_removes_kind_credentials() {
        let mut github_env = BTreeMap::new();
        let mut agent_env = BTreeMap::new();
        github_env.insert(
            "GH_TOKEN".to_string(),
            crate::operator_env::EnvValue::Plain("token".into()),
        );
        agent_env.insert(
            "ANTHROPIC_API_KEY".to_string(),
            crate::operator_env::EnvValue::Plain("key".into()),
        );

        clear_settings_auth_env_values(AuthKind::Github, &mut github_env, &mut agent_env);

        assert!(!github_env.contains_key("GH_TOKEN"));
        assert!(agent_env.contains_key("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn app_github_env_reads_global_github_env() {
        let mut cfg = AppConfig::default();
        assert!(app_github_env(&cfg).is_empty());

        let mut github = GithubAuthConfig::default();
        github.env.insert(
            "GH_TOKEN".to_string(),
            crate::operator_env::EnvValue::Plain("token".into()),
        );
        cfg.github = Some(github.clone());

        assert_eq!(app_github_env(&cfg), github.env);
    }

    #[test]
    fn panel_mode_requires_credential_reads_effective_mode() {
        let cfg = AppConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                ..Default::default()
            }),
            ..AppConfig::default()
        };

        assert!(panel_mode_requires_credential(
            &cfg,
            "workspace",
            "",
            AuthKind::Github
        ));
        assert!(!panel_mode_requires_credential(
            &cfg,
            "workspace",
            "",
            AuthKind::Claude
        ));
    }

    #[test]
    fn eligible_role_keys_for_override_uses_allowed_or_all_roles() {
        let mut cfg = AppConfig::default();
        cfg.roles.insert("alpha".into(), RoleSource::default());
        cfg.roles.insert("beta".into(), RoleSource::default());

        let mut workspace = WorkspaceConfig::default();
        let mut eligible = eligible_role_keys_for_override(&cfg, &workspace);
        eligible.sort();
        assert_eq!(eligible, vec!["alpha".to_string(), "beta".to_string()]);

        workspace.allowed_roles = vec!["ghost".into()];
        assert_eq!(
            eligible_role_keys_for_override(&cfg, &workspace),
            vec!["ghost".to_string()]
        );
    }

    #[test]
    fn settings_auth_env_value_uses_github_or_agent_env() {
        let mut github_env = BTreeMap::new();
        github_env.insert(
            "GH_TOKEN".into(),
            crate::operator_env::EnvValue::Plain("github-token".into()),
        );
        let mut agent_env = BTreeMap::new();
        agent_env.insert(
            AuthKind::Claude
                .required_env_var(AuthMode::ApiKey)
                .expect("Claude API key env var")
                .into(),
            crate::operator_env::EnvValue::Plain("anthropic-key".into()),
        );

        assert!(matches!(
            settings_auth_env_value(AuthKind::Github, AuthMode::Token, &github_env, &agent_env),
            Some(crate::operator_env::EnvValue::Plain(value)) if value == "github-token"
        ));
        assert!(matches!(
            settings_auth_env_value(AuthKind::Claude, AuthMode::ApiKey, &github_env, &agent_env),
            Some(crate::operator_env::EnvValue::Plain(value)) if value == "anthropic-key"
        ));
        assert!(settings_auth_env_value(
            AuthKind::Claude,
            AuthMode::Sync,
            &github_env,
            &agent_env
        )
        .is_none());
    }

    #[test]
    fn workspace_auth_mode_and_credential_reads_workspace_layers() {
        let mut workspace = WorkspaceConfig {
            claude: Some(crate::config::AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
            }),
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                ..Default::default()
            }),
            ..Default::default()
        };
        workspace.env.insert(
            AuthKind::Claude
                .required_env_var(AuthMode::ApiKey)
                .expect("Claude API key env var")
                .into(),
            crate::operator_env::EnvValue::Plain("anthropic-key".into()),
        );
        workspace.github.as_mut().expect("github").env.insert(
            "GH_TOKEN".into(),
            crate::operator_env::EnvValue::Plain("github-token".into()),
        );

        assert!(matches!(
            workspace_auth_mode_and_credential(&workspace, AuthKind::Claude),
            (Some(AuthMode::ApiKey), Some(crate::operator_env::EnvValue::Plain(value)))
                if value == "anthropic-key"
        ));
        assert!(matches!(
            workspace_auth_mode_and_credential(&workspace, AuthKind::Github),
            (Some(AuthMode::Token), Some(crate::operator_env::EnvValue::Plain(value)))
                if value == "github-token"
        ));
    }

    #[test]
    fn role_auth_mode_and_credential_reads_role_layers() {
        let mut role = WorkspaceRoleOverride {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                ..Default::default()
            }),
            ..Default::default()
        };
        role.github.as_mut().expect("github").env.insert(
            "GH_TOKEN".into(),
            crate::operator_env::EnvValue::Plain("github-token".into()),
        );

        assert!(matches!(
            role_auth_mode_and_credential(Some(&role), AuthKind::Github),
            (Some(AuthMode::Token), Some(crate::operator_env::EnvValue::Plain(value)))
                if value == "github-token"
        ));
        assert_eq!(
            role_auth_mode_and_credential(None, AuthKind::Github),
            (None, None)
        );
    }

    #[test]
    fn explicit_workspace_auth_mode_reads_workspace_block() {
        let workspace = WorkspaceConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(
            explicit_workspace_auth_mode(&workspace, AuthKind::Github),
            Some(AuthMode::Token)
        );
        assert_eq!(
            explicit_workspace_auth_mode(&workspace, AuthKind::Claude),
            None
        );
    }

    #[test]
    fn panel_auth_source_value_prefers_workspace_role_then_workspace_then_global() {
        let env_name = AuthKind::Claude
            .required_env_var(AuthMode::ApiKey)
            .expect("Claude API key env var");
        let mut cfg = AppConfig::default();
        cfg.env.insert(
            env_name.into(),
            crate::operator_env::EnvValue::Plain("global".into()),
        );
        let mut workspace = WorkspaceConfig::default();
        workspace.env.insert(
            env_name.into(),
            crate::operator_env::EnvValue::Plain("workspace".into()),
        );
        let mut role = WorkspaceRoleOverride::default();
        role.env.insert(
            env_name.into(),
            crate::operator_env::EnvValue::Plain("workspace-role".into()),
        );
        workspace.roles.insert("smith".into(), role);
        cfg.workspaces.insert("ws".into(), workspace);

        assert!(matches!(
            panel_auth_source_value(&cfg, "ws", "smith", env_name, AuthKind::Claude),
            Some(crate::operator_env::EnvValue::Plain(value)) if value == "workspace-role"
        ));
        assert!(matches!(
            panel_auth_source_value(&cfg, "ws", "", env_name, AuthKind::Claude),
            Some(crate::operator_env::EnvValue::Plain(value)) if value == "workspace"
        ));
        assert!(matches!(
            panel_auth_source_value(&cfg, "missing", "", env_name, AuthKind::Claude),
            Some(crate::operator_env::EnvValue::Plain(value)) if value == "global"
        ));
    }

    #[test]
    fn panel_auth_source_value_uses_github_env_layers() {
        let mut cfg = AppConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                ..Default::default()
            }),
            ..Default::default()
        };
        cfg.github.as_mut().expect("github").env.insert(
            "GH_TOKEN".into(),
            crate::operator_env::EnvValue::Plain("global-gh".into()),
        );
        let mut workspace = WorkspaceConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                ..Default::default()
            }),
            ..Default::default()
        };
        workspace.github.as_mut().expect("github").env.insert(
            "GH_TOKEN".into(),
            crate::operator_env::EnvValue::Plain("workspace-gh".into()),
        );
        cfg.workspaces.insert("ws".into(), workspace);

        assert!(matches!(
            panel_auth_source_value(&cfg, "ws", "", "GH_TOKEN", AuthKind::Github),
            Some(crate::operator_env::EnvValue::Plain(value)) if value == "workspace-gh"
        ));
        assert!(matches!(
            panel_auth_source_value(&cfg, "missing", "", "GH_TOKEN", AuthKind::Github),
            Some(crate::operator_env::EnvValue::Plain(value)) if value == "global-gh"
        ));
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
