//! Auth configuration edit helpers shared by console surfaces.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, EnvValue, GithubAuthConfig, GithubAuthMode,
    WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_core::{Agent, env_model};

use crate::tui::auth::{AuthKind, AuthMode};
use crate::tui::components::editor_rows::{AuthSourceFolderDisplay, AuthSourceFolderKind};
use crate::tui::screens::settings::model::SettingsAuthRow;

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
}

pub fn clear_role_auth_layer(role: &mut WorkspaceRoleOverride, kind: AuthKind) {
    set_role_auth_mode(role, kind, None);
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

#[allow(clippy::missing_const_for_fn)]
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
