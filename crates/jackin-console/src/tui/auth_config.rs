// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Auth configuration edit helpers shared by console surfaces.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, EnvValue, GithubAuthConfig, GithubAuthMode,
    WorkspaceConfig, WorkspaceRoleOverride, resolve_github_mode, resolve_mode,
};
use jackin_core::{Agent, env_model};

use crate::tui::auth::{AuthKind, AuthMode, can_generate_claude_oauth_token};
use crate::tui::components::auth_panel::{AuthCredential, AuthForm};
use crate::tui::components::editor_rows::{AuthSourceFolderDisplay, AuthSourceFolderKind};
use crate::tui::screens::settings::model::{AuthFormTarget, SettingsAuthRow};

/// Merge live global auth/env/role config with a pending workspace edit so
/// auth views resolve against unsaved workspace-local changes.
#[must_use]
pub fn synthesize_app_config_for_workspace_auth(
    config: &AppConfig,
    workspace_name: String,
    pending_workspace: WorkspaceConfig,
) -> AppConfig {
    let mut synthesized = AppConfig {
        claude: config.claude.clone(),
        codex: config.codex.clone(),
        amp: config.amp.clone(),
        opencode: config.opencode.clone(),
        github: config.github.clone(),
        env: config.env.clone(),
        roles: config.roles.clone(),
        ..AppConfig::default()
    };
    synthesized
        .workspaces
        .insert(workspace_name, pending_workspace);
    synthesized
}

#[must_use]
pub const fn auth_kind_agent(kind: AuthKind) -> Option<Agent> {
    match kind {
        AuthKind::Claude => Some(Agent::Claude),
        AuthKind::Codex => Some(Agent::Codex),
        AuthKind::Amp => Some(Agent::Amp),
        AuthKind::Kimi => Some(Agent::Kimi),
        AuthKind::Opencode => Some(Agent::Opencode),
        AuthKind::Grok => Some(Agent::Grok),
        AuthKind::Github | AuthKind::Zai | AuthKind::Minimax => None,
    }
}

#[must_use]
pub fn role_override_present(kind: AuthKind, ro: &WorkspaceRoleOverride) -> bool {
    match kind {
        AuthKind::Claude => ro.claude.is_some(),
        AuthKind::Codex => ro.codex.is_some(),
        AuthKind::Amp => ro.amp.is_some(),
        // Kimi covers both the typed agent block and env-key-based provider routing.
        AuthKind::Kimi => {
            ro.kimi.is_some() || ro.env.contains_key(env_model::KIMI_CODE_API_KEY_ENV_NAME)
        }
        AuthKind::Opencode => ro.opencode.is_some(),
        AuthKind::Grok => ro.grok.is_some(),
        AuthKind::Github => ro.github.is_some(),
        AuthKind::Zai => ro.env.contains_key(env_model::ZAI_API_KEY_ENV_NAME),
        // Minimax is env-only; no typed block in WorkspaceRoleOverride.
        AuthKind::Minimax => ro.env.contains_key(env_model::MINIMAX_API_KEY_ENV_NAME),
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
pub const fn auth_mode_from_auth_forward(mode: AuthForwardMode) -> AuthMode {
    match mode {
        AuthForwardMode::Sync => AuthMode::Sync,
        AuthForwardMode::ApiKey => AuthMode::ApiKey,
        AuthForwardMode::OAuthToken => AuthMode::OAuthToken,
        AuthForwardMode::Ignore => AuthMode::Ignore,
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
pub const fn auth_mode_from_github(mode: GithubAuthMode) -> AuthMode {
    match mode {
        GithubAuthMode::Sync => AuthMode::Sync,
        GithubAuthMode::Token => AuthMode::Token,
        GithubAuthMode::Ignore => AuthMode::Ignore,
    }
}

#[must_use]
pub fn editor_auth_form_can_generate_token(
    editing_existing_workspace: bool,
    target: &AuthFormTarget<AuthKind>,
    kind: AuthKind,
    mode: Option<AuthMode>,
) -> bool {
    editing_existing_workspace
        && can_generate_claude_oauth_token(kind, mode)
        && matches!(
            target,
            AuthFormTarget::Workspace {
                kind: AuthKind::Claude
            } | AuthFormTarget::WorkspaceRole {
                kind: AuthKind::Claude,
                ..
            }
        )
}

pub trait AuthFormGenerateTarget {
    fn can_generate_claude_token_target(&self) -> bool;
}

impl AuthFormGenerateTarget for AuthFormTarget<AuthKind> {
    fn can_generate_claude_token_target(&self) -> bool {
        matches!(
            self,
            AuthFormTarget::Workspace {
                kind: AuthKind::Claude
            } | AuthFormTarget::WorkspaceRole {
                kind: AuthKind::Claude,
                ..
            }
        )
    }
}

pub trait AuthFormGenerateState {
    fn generate_kind(&self) -> AuthKind;
    fn generate_mode(&self) -> Option<AuthMode>;
}

impl<V: AuthCredential> AuthFormGenerateState for AuthForm<V> {
    fn generate_kind(&self) -> AuthKind {
        self.kind
    }

    fn generate_mode(&self) -> Option<AuthMode> {
        self.mode
    }
}

#[must_use]
pub fn auth_form_generate_eligible(
    editing_existing_workspace: bool,
    target: &impl AuthFormGenerateTarget,
    form: &impl AuthFormGenerateState,
) -> bool {
    editing_existing_workspace
        && target.can_generate_claude_token_target()
        && can_generate_claude_oauth_token(form.generate_kind(), form.generate_mode())
}

pub trait ModalAuthFormGenerate {
    fn auth_form_can_generate_token(&self, editing_existing_workspace: bool) -> bool;
}

pub trait ModalAuthFormFocusInspect<AuthFormFocus> {
    fn active_auth_form_focus(&self) -> Option<AuthFormFocus>;
}

pub trait ModalAuthFormParentInspect {
    fn is_auth_form_parent(&self) -> bool;
}

pub trait ModalAuthTokenGenerateStart<Target, SourcePickerState>: Sized {
    fn open_auth_generate_source_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        source_picker_state: SourcePickerState,
    ) -> Option<Target>;
}

pub trait ModalAuthPlainSourceOpen<TextInputTarget, TextInputState, AuthFormFocus>: Sized {
    fn open_auth_plain_source_text_input(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        credential_focus: AuthFormFocus,
        text_input_target: TextInputTarget,
        make_text_input: impl FnOnce(String) -> TextInputState,
    ) -> bool;
}

pub trait ModalAuthOpPickerOpen<OpPickerState, AuthFormFocus>: Sized {
    fn open_auth_op_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        credential_focus: AuthFormFocus,
        make_op_picker: impl FnOnce() -> OpPickerState,
    ) -> bool;
}

pub trait AuthFormCredentialEdit {
    type OpRef;

    fn set_auth_literal(&mut self, value: String);
    fn set_auth_source_folder(&mut self, value: PathBuf);
    fn set_auth_op_ref(&mut self, value: Self::OpRef);
}

impl<V: AuthCredential> AuthFormCredentialEdit for AuthForm<V> {
    type OpRef = V::Ref;

    fn set_auth_literal(&mut self, value: String) {
        self.set_literal(value);
    }

    fn set_auth_source_folder(&mut self, value: PathBuf) {
        self.set_source_folder(value);
    }

    fn set_auth_op_ref(&mut self, value: Self::OpRef) {
        self.set_op_ref(value);
    }
}

pub trait ModalAuthFormCredentialApply<AuthFormFocus>: Sized {
    fn apply_auth_plain_text(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: &str,
    ) -> bool;

    fn apply_auth_source_folder(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: PathBuf,
    ) -> bool;

    fn restore_auth_form_modal(modal: &mut Option<Self>, modal_parents: &mut Vec<Self>) -> bool;
}

pub trait ModalAuthFormOpRefApply<AuthFormFocus, OpRef>: Sized {
    fn apply_auth_op_ref(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: OpRef,
    ) -> bool;
}

pub trait AuthFormCredentialSourceState {
    fn required_credential_env_var(&self) -> Option<&'static str>;
}

impl<V: AuthCredential> AuthFormCredentialSourceState for AuthForm<V> {
    fn required_credential_env_var(&self) -> Option<&'static str> {
        self.mode.and_then(|mode| self.kind.required_env_var(mode))
    }
}

pub trait ModalAuthSourcePickerOpen<SourcePickerState>: Sized {
    fn open_auth_source_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        make_source_picker: impl FnOnce(&'static str) -> SourcePickerState,
    ) -> bool;
}

pub trait AuthFormSourceFolderState {
    fn shows_auth_source_folder(&self) -> bool;
}

impl<V: AuthCredential> AuthFormSourceFolderState for AuthForm<V> {
    fn shows_auth_source_folder(&self) -> bool {
        self.shows_source_folder()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceFolderBrowserOpenResult<E> {
    Opened,
    NotAvailable,
    BrowserError(E),
}

pub trait ModalAuthSourceFolderBrowserOpen<FileBrowserTarget, FileBrowserState, AuthFormFocus>:
    Sized
{
    fn open_auth_source_folder_browser<E>(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        source_folder_focus: AuthFormFocus,
        file_browser_target: FileBrowserTarget,
        make_browser: impl FnOnce() -> Result<FileBrowserState, E>,
    ) -> AuthSourceFolderBrowserOpenResult<E>;
}

#[must_use]
pub const fn settings_auth_form_can_generate_token(kind: AuthKind, mode: Option<AuthMode>) -> bool {
    can_generate_claude_oauth_token(kind, mode)
}

#[must_use]
pub fn github_auth_config_with_preserved_env(
    mode: Option<AuthMode>,
    existing: Option<&GithubAuthConfig>,
) -> Option<GithubAuthConfig> {
    mode.and_then(auth_mode_to_github)
        .map(|auth_forward| GithubAuthConfig {
            auth_forward,
            env: existing
                .map(|github| github.env.clone())
                .unwrap_or_default(),
        })
}

pub fn set_workspace_auth_mode(ws: &mut WorkspaceConfig, kind: AuthKind, mode: Option<AuthMode>) {
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
    fn claude_auth(&self) -> Option<&AgentAuthConfig>;
    fn codex_auth(&self) -> Option<&AgentAuthConfig>;
    fn amp_auth(&self) -> Option<&AgentAuthConfig>;
    fn kimi_auth(&self) -> Option<&AgentAuthConfig>;
    fn opencode_auth(&self) -> Option<&AgentAuthConfig>;
    fn grok_auth(&self) -> Option<&AgentAuthConfig>;
    fn github_auth(&self) -> Option<&GithubAuthConfig>;
    fn set_claude_auth(&mut self, auth: Option<AgentAuthConfig>);
    fn set_codex_auth(&mut self, auth: Option<AgentAuthConfig>);
    fn set_amp_auth(&mut self, auth: Option<AgentAuthConfig>);
    fn set_kimi_auth(&mut self, auth: Option<AgentAuthConfig>);
    fn set_opencode_auth(&mut self, auth: Option<AgentAuthConfig>);
    fn set_grok_auth(&mut self, auth: Option<AgentAuthConfig>);
    fn set_github_auth(&mut self, auth: Option<GithubAuthConfig>);
}

impl AuthLayerMut for WorkspaceConfig {
    fn claude_auth(&self) -> Option<&AgentAuthConfig> {
        self.claude.as_ref()
    }

    fn codex_auth(&self) -> Option<&AgentAuthConfig> {
        self.codex.as_ref()
    }

    fn amp_auth(&self) -> Option<&AgentAuthConfig> {
        self.amp.as_ref()
    }

    fn kimi_auth(&self) -> Option<&AgentAuthConfig> {
        self.kimi.as_ref()
    }

    fn opencode_auth(&self) -> Option<&AgentAuthConfig> {
        self.opencode.as_ref()
    }

    fn grok_auth(&self) -> Option<&AgentAuthConfig> {
        self.grok.as_ref()
    }

    fn github_auth(&self) -> Option<&GithubAuthConfig> {
        self.github.as_ref()
    }

    fn set_claude_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.claude = auth;
    }

    fn set_codex_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.codex = auth;
    }

    fn set_amp_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.amp = auth;
    }

    fn set_kimi_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.kimi = auth;
    }

    fn set_opencode_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.opencode = auth;
    }

    fn set_grok_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.grok = auth;
    }

    fn set_github_auth(&mut self, auth: Option<GithubAuthConfig>) {
        self.github = auth;
    }
}

impl AuthLayerMut for WorkspaceRoleOverride {
    fn claude_auth(&self) -> Option<&AgentAuthConfig> {
        self.claude.as_ref()
    }

    fn codex_auth(&self) -> Option<&AgentAuthConfig> {
        self.codex.as_ref()
    }

    fn amp_auth(&self) -> Option<&AgentAuthConfig> {
        self.amp.as_ref()
    }

    fn kimi_auth(&self) -> Option<&AgentAuthConfig> {
        self.kimi.as_ref()
    }

    fn opencode_auth(&self) -> Option<&AgentAuthConfig> {
        self.opencode.as_ref()
    }

    fn grok_auth(&self) -> Option<&AgentAuthConfig> {
        self.grok.as_ref()
    }

    fn github_auth(&self) -> Option<&GithubAuthConfig> {
        self.github.as_ref()
    }

    fn set_claude_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.claude = auth;
    }

    fn set_codex_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.codex = auth;
    }

    fn set_amp_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.amp = auth;
    }

    fn set_kimi_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.kimi = auth;
    }

    fn set_opencode_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.opencode = auth;
    }

    fn set_grok_auth(&mut self, auth: Option<AgentAuthConfig>) {
        self.grok = auth;
    }

    fn set_github_auth(&mut self, auth: Option<GithubAuthConfig>) {
        self.github = auth;
    }
}

fn set_auth_mode(layer: &mut impl AuthLayerMut, kind: AuthKind, mode: Option<AuthMode>) {
    match kind {
        AuthKind::Claude => {
            layer.set_claude_auth(agent_auth_config_with_preserved_source(
                mode,
                layer.claude_auth(),
            ));
        }
        AuthKind::Codex => {
            layer.set_codex_auth(agent_auth_config_with_preserved_source(
                mode,
                layer.codex_auth(),
            ));
        }
        AuthKind::Amp => {
            layer.set_amp_auth(agent_auth_config_with_preserved_source(
                mode,
                layer.amp_auth(),
            ));
        }
        AuthKind::Kimi => {
            layer.set_kimi_auth(agent_auth_config_with_preserved_source(
                mode,
                layer.kimi_auth(),
            ));
        }
        AuthKind::Opencode => {
            layer.set_opencode_auth(agent_auth_config_with_preserved_source(
                mode,
                layer.opencode_auth(),
            ));
        }
        AuthKind::Grok => {
            layer.set_grok_auth(agent_auth_config_with_preserved_source(
                mode,
                layer.grok_auth(),
            ));
        }
        AuthKind::Github => {
            layer.set_github_auth(github_auth_config_with_preserved_env(
                mode,
                layer.github_auth(),
            ));
        }
        AuthKind::Zai | AuthKind::Minimax => {}
    }
}

fn agent_auth_config_with_preserved_source(
    mode: Option<AuthMode>,
    existing: Option<&AgentAuthConfig>,
) -> Option<AgentAuthConfig> {
    mode.and_then(auth_mode_to_auth_forward)
        .map(|auth_forward| AgentAuthConfig {
            auth_forward,
            sync_source_dir: existing.and_then(|cfg| cfg.sync_source_dir.clone()),
        })
}

fn set_agent_sync_source_dir(
    existing: Option<&AgentAuthConfig>,
    source: Option<PathBuf>,
) -> Option<AgentAuthConfig> {
    let auth_forward = existing.map_or(AuthForwardMode::Sync, |cfg| cfg.auth_forward);
    if source.is_none() && existing.is_none() {
        return None;
    }
    let cfg = AgentAuthConfig {
        auth_forward,
        sync_source_dir: source,
    };
    if cfg == AgentAuthConfig::default() {
        None
    } else {
        Some(cfg)
    }
}

fn set_sync_source_dir(layer: &mut impl AuthLayerMut, kind: AuthKind, source: Option<PathBuf>) {
    match kind {
        AuthKind::Claude => {
            layer.set_claude_auth(set_agent_sync_source_dir(layer.claude_auth(), source));
        }
        AuthKind::Codex => {
            layer.set_codex_auth(set_agent_sync_source_dir(layer.codex_auth(), source));
        }
        AuthKind::Amp => {
            layer.set_amp_auth(set_agent_sync_source_dir(layer.amp_auth(), source));
        }
        AuthKind::Kimi => {
            layer.set_kimi_auth(set_agent_sync_source_dir(layer.kimi_auth(), source));
        }
        AuthKind::Opencode => {
            layer.set_opencode_auth(set_agent_sync_source_dir(layer.opencode_auth(), source));
        }
        AuthKind::Grok => {
            layer.set_grok_auth(set_agent_sync_source_dir(layer.grok_auth(), source));
        }
        AuthKind::Github | AuthKind::Zai | AuthKind::Minimax => {}
    }
}

pub fn set_workspace_sync_source_dir(
    ws: &mut WorkspaceConfig,
    kind: AuthKind,
    source: Option<PathBuf>,
) {
    set_sync_source_dir(ws, kind, source);
}

pub fn set_role_sync_source_dir(
    role: &mut WorkspaceRoleOverride,
    kind: AuthKind,
    source: Option<PathBuf>,
) {
    set_sync_source_dir(role, kind, source);
}

pub fn apply_workspace_auth_commit(
    ws: &mut WorkspaceConfig,
    kind: AuthKind,
    mode: AuthMode,
    env_var_name: Option<&str>,
    env_value: Option<EnvValue>,
) {
    set_workspace_auth_mode(ws, kind, Some(mode));
    if mode == AuthMode::Ignore
        && matches!(kind, AuthKind::Zai | AuthKind::Minimax)
        && let Some(env_key) = kind.required_env_var(AuthMode::ApiKey)
    {
        ws.env.remove(env_key);
    }
    apply_auth_env_value(
        &mut ws.env,
        ws.github.as_mut(),
        kind,
        env_var_name,
        env_value,
    );
}

pub fn apply_role_auth_commit(
    role: &mut WorkspaceRoleOverride,
    kind: AuthKind,
    mode: AuthMode,
    env_var_name: Option<&str>,
    env_value: Option<EnvValue>,
) {
    set_role_auth_mode(role, kind, Some(mode));
    if mode == AuthMode::Ignore
        && matches!(kind, AuthKind::Zai | AuthKind::Minimax)
        && let Some(env_key) = kind.required_env_var(AuthMode::ApiKey)
    {
        role.env.remove(env_key);
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
    clear_workspace_env_only_auth_values(ws, kind);
}

pub fn clear_role_auth_layer(role: &mut WorkspaceRoleOverride, kind: AuthKind) {
    set_role_auth_mode(role, kind, None);
    clear_role_env_only_auth_values(role, kind);
}

fn clear_workspace_env_only_auth_values(ws: &mut WorkspaceConfig, kind: AuthKind) {
    for mode in kind.supported_modes() {
        if let Some(env_var) = kind.required_env_var(*mode) {
            ws.env.remove(env_var);
        }
    }
}

fn clear_role_env_only_auth_values(role: &mut WorkspaceRoleOverride, kind: AuthKind) {
    for mode in kind.supported_modes() {
        if let Some(env_var) = kind.required_env_var(*mode) {
            role.env.remove(env_var);
        }
    }
}

pub fn apply_settings_auth_env_commit(
    kind: AuthKind,
    env_var_name: Option<&str>,
    env_value: Option<EnvValue>,
    github_env: &mut BTreeMap<String, EnvValue>,
    agent_env: &mut BTreeMap<String, EnvValue>,
) {
    let (Some(name), Some(value)) = (env_var_name, env_value) else {
        return;
    };
    settings_auth_env_map_mut(kind, github_env, agent_env).insert(name.to_owned(), value);
}

pub fn clear_settings_auth_env_values(
    kind: AuthKind,
    github_env: &mut BTreeMap<String, EnvValue>,
    agent_env: &mut BTreeMap<String, EnvValue>,
) {
    for mode in kind.supported_modes() {
        if let Some(env_var) = kind.required_env_var(*mode) {
            settings_auth_env_map_mut(kind, github_env, agent_env).remove(env_var);
        }
    }
}

pub fn clear_ignored_env_only_settings_auth_keys(
    rows: &[SettingsAuthRow<AuthKind, AuthMode>],
    agent_env: &mut BTreeMap<String, EnvValue>,
) {
    for row in rows {
        if row.mode == AuthMode::Ignore
            && matches!(row.kind, AuthKind::Zai | AuthKind::Minimax)
            && let Some(env_key) = row.kind.required_env_var(AuthMode::ApiKey)
        {
            agent_env.remove(env_key);
        }
    }
}

#[must_use]
pub fn env_display_map(values: &BTreeMap<String, EnvValue>) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.clone(), value.as_display_str().to_owned()))
        .collect()
}

#[must_use]
pub fn env_display_map_without_auth_credentials(
    values: &BTreeMap<String, EnvValue>,
) -> BTreeMap<String, String> {
    let credential_keys = auth_credential_env_keys();
    values
        .iter()
        .filter(|(key, _)| !credential_keys.contains(key.as_str()))
        .map(|(key, value)| (key.clone(), value.as_display_str().to_owned()))
        .collect()
}

#[must_use]
pub fn auth_credential_env_keys() -> BTreeSet<&'static str> {
    AuthKind::SETTINGS_KINDS
        .iter()
        .flat_map(|kind| {
            kind.supported_modes()
                .iter()
                .filter_map(|mode| kind.required_env_var(*mode))
        })
        .collect()
}

#[expect(
    clippy::missing_const_for_fn,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
fn settings_auth_env_map_mut<'a>(
    kind: AuthKind,
    github_env: &'a mut BTreeMap<String, EnvValue>,
    agent_env: &'a mut BTreeMap<String, EnvValue>,
) -> &'a mut BTreeMap<String, EnvValue> {
    match kind {
        AuthKind::Github => github_env,
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode
        | AuthKind::Grok
        | AuthKind::Zai
        | AuthKind::Minimax => agent_env,
    }
}

fn apply_auth_env_value(
    env: &mut BTreeMap<String, EnvValue>,
    github: Option<&mut GithubAuthConfig>,
    kind: AuthKind,
    env_var_name: Option<&str>,
    env_value: Option<EnvValue>,
) {
    let (Some(name), Some(value)) = (env_var_name, env_value) else {
        return;
    };
    match kind {
        AuthKind::Github => {
            if let Some(github) = github {
                github.env.insert(name.to_owned(), value);
            }
        }
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode
        | AuthKind::Grok
        | AuthKind::Zai
        | AuthKind::Minimax => {
            env.insert(name.to_owned(), value);
        }
    }
}

#[must_use]
pub fn app_github_env(cfg: &AppConfig) -> BTreeMap<String, EnvValue> {
    cfg.github
        .as_ref()
        .map(|github| github.env.clone())
        .unwrap_or_default()
}

#[must_use]
pub fn settings_auth_rows_from_app_config(
    cfg: &AppConfig,
) -> Vec<SettingsAuthRow<AuthKind, AuthMode>> {
    AuthKind::SETTINGS_KINDS
        .iter()
        .copied()
        .map(|kind| SettingsAuthRow {
            kind,
            mode: resolve_panel_mode(cfg, kind, "", ""),
            sync_source_dir: auth_kind_agent(kind).and_then(|agent| cfg.sync_source_dir_for(agent)),
        })
        .collect()
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

#[must_use]
pub fn settings_auth_env_value<'a>(
    kind: AuthKind,
    mode: AuthMode,
    github_env: &'a BTreeMap<String, EnvValue>,
    agent_env: &'a BTreeMap<String, EnvValue>,
) -> Option<&'a EnvValue> {
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
) -> (Option<AuthMode>, Option<EnvValue>) {
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
            env_only_mode_and_credential(&workspace.env, env_model::ZAI_API_KEY_ENV_NAME)
        }
        AuthKind::Minimax => {
            env_only_mode_and_credential(&workspace.env, env_model::MINIMAX_API_KEY_ENV_NAME)
        }
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
) -> Option<&'a EnvValue> {
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
) -> Option<&'a EnvValue> {
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
) -> Option<&'a EnvValue> {
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
) -> (Option<AuthMode>, Option<EnvValue>) {
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
            env_only_mode_and_credential(&role.env, env_model::ZAI_API_KEY_ENV_NAME)
        }),
        AuthKind::Minimax => role.map_or((None, None), |role| {
            env_only_mode_and_credential(&role.env, env_model::MINIMAX_API_KEY_ENV_NAME)
        }),
    }
}

fn agent_workspace_mode_and_credential(
    auth_forward: Option<AuthForwardMode>,
    env: &BTreeMap<String, EnvValue>,
    kind: AuthKind,
) -> (Option<AuthMode>, Option<EnvValue>) {
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
) -> (Option<AuthMode>, Option<EnvValue>) {
    let mode = auth_forward.map(auth_mode_from_auth_forward);
    let credential = mode
        .and_then(|mode| kind.required_env_var(mode))
        .and_then(|name| role.and_then(|role| role.env.get(name).cloned()));
    (mode, credential)
}

fn env_only_mode_and_credential(
    env: &BTreeMap<String, EnvValue>,
    key: &str,
) -> (Option<AuthMode>, Option<EnvValue>) {
    let credential = env.get(key).cloned();
    let mode = credential.as_ref().map(|_| AuthMode::ApiKey);
    (mode, credential)
}

/// Resolve the effective auth mode for the panel via the kind-specific
/// resolver in `jackin_config`. Agent kinds go through `resolve_mode`;
/// Github routes through `resolve_github_mode`.
#[must_use]
pub fn resolve_panel_mode(
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
            let ws = jackin_core::WorkspaceName::parse(workspace).ok();
            let mode = resolve_mode(cfg, agent, ws.as_ref(), role);
            auth_mode_from_auth_forward(mode)
        }
        AuthKind::Github => {
            let ws = jackin_core::WorkspaceName::parse(workspace).ok();
            let mode = resolve_github_mode(cfg, ws.as_ref(), role);
            auth_mode_from_github(mode)
        }
        AuthKind::Zai | AuthKind::Minimax => {
            let Some(env_key) = kind.required_env_var(AuthMode::ApiKey) else {
                return AuthMode::Ignore;
            };
            let key_present = operator_env_value(
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

fn operator_env_value<'a>(
    cfg: &'a AppConfig,
    role: Option<&str>,
    workspace: Option<&str>,
    key: &str,
) -> Option<&'a EnvValue> {
    let mut value = cfg.env.get(key);
    if let Some(role_name) = role
        && let Some(role_source) = cfg.roles.get(role_name)
        && let Some(role_value) = role_source.env.get(key)
    {
        value = Some(role_value);
    }
    if let Some(workspace_name) = workspace
        && let Some(workspace_cfg) = cfg.workspaces.get(workspace_name)
    {
        if let Some(workspace_value) = workspace_cfg.env.get(key) {
            value = Some(workspace_value);
        }
        if let Some(role_name) = role
            && let Some(role_value) = workspace_cfg
                .roles
                .get(role_name)
                .and_then(|role| role.env.get(key))
        {
            value = Some(role_value);
        }
    }
    value
}

#[must_use]
pub fn settings_source_folder_display(
    row: &SettingsAuthRow<AuthKind, AuthMode>,
) -> AuthSourceFolderDisplay {
    let Some(agent) = auth_kind_agent(row.kind) else {
        return AuthSourceFolderDisplay {
            kind: AuthSourceFolderKind::Default,
            path: String::new(),
        };
    };
    let paths = agent.runtime().state_paths();
    AuthSourceFolderDisplay {
        kind: row
            .sync_source_dir
            .as_ref()
            .map_or(AuthSourceFolderKind::Default, |_| {
                AuthSourceFolderKind::Explicit
            }),
        path: row.sync_source_dir.as_ref().map_or_else(
            || format!("~/{}", paths.credential_dir),
            |path| path.display().to_string(),
        ),
    }
}

#[must_use]
pub fn editor_source_folder_display(
    config: &AppConfig,
    workspace_name: &str,
    role: &str,
    kind: AuthKind,
) -> AuthSourceFolderDisplay {
    let Some(agent) = auth_kind_agent(kind) else {
        return AuthSourceFolderDisplay {
            kind: AuthSourceFolderKind::Default,
            path: String::new(),
        };
    };
    let paths = agent.runtime().state_paths();
    let ws = config.workspaces.get(workspace_name);
    let role_value = if role.is_empty() {
        None
    } else {
        ws.and_then(|ws| ws.roles.get(role))
            .and_then(|role| role.sync_source_dir_for(agent))
    };
    let workspace_value = ws.and_then(|ws| ws.sync_source_dir_for(agent));
    let global_value = config.sync_source_dir_for(agent);
    let (kind, path) = if let Some(path) = role_value {
        (AuthSourceFolderKind::Explicit, path.display().to_string())
    } else if role.is_empty() {
        if let Some(path) = workspace_value {
            (AuthSourceFolderKind::Explicit, path.display().to_string())
        } else if let Some(path) = global_value {
            (AuthSourceFolderKind::Inherited, path.display().to_string())
        } else {
            (
                AuthSourceFolderKind::Default,
                format!("~/{}", paths.credential_dir),
            )
        }
    } else if let Some(path) = workspace_value.or(global_value) {
        (AuthSourceFolderKind::Inherited, path.display().to_string())
    } else {
        (
            AuthSourceFolderKind::Default,
            format!("~/{}", paths.credential_dir),
        )
    };
    AuthSourceFolderDisplay { kind, path }
}

#[cfg(test)]
mod tests;
