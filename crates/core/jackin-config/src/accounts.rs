// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::schema::WorkspaceConfig;
use crate::{AppConfig, ConfigError, ConfigResult};
use jackin_core::{
    AMP_API_KEY_ENV_NAME, ANTHROPIC_API_KEY_ENV_NAME, ANTHROPIC_AUTH_TOKEN_ENV_NAME, Agent,
    AuthForwardMode, CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME, CURSOR_API_KEY_ENV_NAME, EnvValue,
    GEMINI_API_KEY_ENV_NAME, KIMI_API_KEY_ENV_NAME, META_API_KEY_ENV_NAME,
    MINIMAX_API_KEY_ENV_NAME, MOONSHOT_API_KEY_ENV_NAME, OPENAI_API_KEY_ENV_NAME,
    OPENCODE_API_KEY_ENV_NAME, OPENROUTER_API_KEY_ENV_NAME, WorkspaceName, XAI_API_KEY_ENV_NAME,
    ZHIPU_API_KEY_ENV_NAME,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(crate) mod discovery;
pub(crate) mod stores;
pub(crate) mod zshrc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    Amp,
    Xai,
    Opencode,
    Moonshot,
    Zai,
    Minimax,
    Google,
    Cursor,
    Meta,
    #[serde(rename = "openrouter")]
    OpenRouter,
}
impl AiProvider {
    pub const ALL: &'static [Self] = &[
        Self::Anthropic,
        Self::OpenAi,
        Self::Amp,
        Self::Xai,
        Self::Opencode,
        Self::Moonshot,
        Self::Zai,
        Self::Minimax,
        Self::Google,
        Self::Cursor,
        Self::Meta,
        Self::OpenRouter,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Amp => "amp",
            Self::Xai => "xai",
            Self::Opencode => "opencode",
            Self::Moonshot => "moonshot",
            Self::Zai => "zai",
            Self::Minimax => "minimax",
            Self::Google => "google",
            Self::Cursor => "cursor",
            Self::Meta => "meta",
            Self::OpenRouter => "openrouter",
        }
    }
    pub const fn for_agent(agent: Agent) -> Option<Self> {
        match agent {
            Agent::Claude => Some(Self::Anthropic),
            Agent::Codex => Some(Self::OpenAi),
            Agent::Amp => Some(Self::Amp),
            Agent::Kimi => Some(Self::Moonshot),
            Agent::Opencode => Some(Self::Opencode),
            Agent::Grok => Some(Self::Xai),
            Agent::Antigravity | Agent::Gemini => Some(Self::Google),
            Agent::Cursor => Some(Self::Cursor),
            Agent::Muse => Some(Self::Meta),
            Agent::Omp | Agent::Hermes => None,
        }
    }
}
impl std::fmt::Display for AiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.slug())
    }
}
impl std::str::FromStr for AiProvider {
    type Err = ConfigError;
    fn from_str(value: &str) -> ConfigResult<Self> {
        match value {
            "anthropic" => Ok(Self::Anthropic),
            "openai" => Ok(Self::OpenAi),
            "amp" => Ok(Self::Amp),
            "xai" => Ok(Self::Xai),
            "opencode" => Ok(Self::Opencode),
            "moonshot" => Ok(Self::Moonshot),
            "zai" => Ok(Self::Zai),
            "minimax" => Ok(Self::Minimax),
            "google" => Ok(Self::Google),
            "cursor" => Ok(Self::Cursor),
            "meta" => Ok(Self::Meta),
            "openrouter" => Ok(Self::OpenRouter),
            _ => Err(ConfigError::msg(format_args!(
                "unknown AI provider {value:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSelector {
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

impl ProfileSelector {
    fn validate(&self, id: &str) -> ConfigResult<()> {
        if self.entry.trim().is_empty() || self.entry.contains('\0') {
            return Err(ConfigError::msg(format_args!(
                "account {id:?} has an invalid profile-store entry selector"
            )));
        }
        if self
            .profile
            .as_deref()
            .is_some_and(|profile| profile.trim().is_empty() || profile.contains('\0'))
        {
            return Err(ConfigError::msg(format_args!(
                "account {id:?} has an invalid profile-store profile selector"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountCredential {
    Profile {
        agent: Agent,
        directory: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        xdg_roots: Option<XdgRoots>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_selector: Option<ProfileSelector>,
    },
    ApiKey {
        value: EnvValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    OAuthToken {
        agent: Agent,
        value: EnvValue,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XdgRoots {
    pub data: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrapperSpec {
    pub identity: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

impl std::fmt::Debug for AccountCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Profile {
                agent,
                directory,
                xdg_roots,
                source_selector,
            } => f
                .debug_struct("Profile")
                .field("agent", agent)
                .field("directory", directory)
                .field("xdg_roots", xdg_roots)
                .field("source_selector", source_selector)
                .finish(),
            Self::ApiKey { .. } => f.write_str("ApiKey { value: [REDACTED] }"),
            Self::OAuthToken { agent, .. } => f
                .debug_struct("OAuthToken")
                .field("agent", agent)
                .field("value", &"[REDACTED]")
                .finish(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedCredentialDescriptor {
    pub agent: Agent,
    pub provider: AiProvider,
    pub mode: AuthForwardMode,
    pub env_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountConfig {
    #[serde(
        default = "account_enabled_by_default",
        skip_serializing_if = "crate::schema::bool_matches::<true>"
    )]
    pub enabled: bool,
    pub name: String,
    pub provider: AiProvider,
    pub credential: AccountCredential,
}
const fn account_enabled_by_default() -> bool {
    true
}
impl AccountConfig {
    pub fn supports_agent(&self, agent: Agent) -> bool {
        self.enabled && self.compatible_agent(agent)
    }

    fn compatible_agent(&self, agent: Agent) -> bool {
        match &self.credential {
            AccountCredential::Profile { agent: owner, .. } => {
                *owner == agent
                    && (Some(self.provider) == AiProvider::for_agent(agent)
                        || matches!(agent, Agent::Opencode | Agent::Omp | Agent::Hermes))
            }
            AccountCredential::OAuthToken { agent: owner, .. } => {
                *owner == agent && agent == Agent::Claude && self.provider == AiProvider::Anthropic
            }
            AccountCredential::ApiKey { .. } => {
                Some(self.provider) == AiProvider::for_agent(agent)
                    || match agent {
                        Agent::Claude => matches!(
                            self.provider,
                            AiProvider::Moonshot | AiProvider::Zai | AiProvider::Minimax
                        ),
                        Agent::Codex => {
                            matches!(
                                self.provider,
                                AiProvider::Moonshot | AiProvider::Zai | AiProvider::Minimax
                            )
                        }
                        Agent::Opencode | Agent::Omp | Agent::Hermes => {
                            !matches!(self.provider, AiProvider::Amp)
                        }
                        _ => false,
                    }
            }
        }
    }
    const fn api_key_variable(&self, agent: Agent) -> &'static str {
        match agent {
            Agent::Claude if !matches!(self.provider, AiProvider::Anthropic) => {
                ANTHROPIC_AUTH_TOKEN_ENV_NAME
            }
            Agent::Claude => ANTHROPIC_API_KEY_ENV_NAME,
            Agent::Codex if matches!(self.provider, AiProvider::Moonshot) => KIMI_API_KEY_ENV_NAME,
            Agent::Codex if matches!(self.provider, AiProvider::Minimax) => {
                MINIMAX_API_KEY_ENV_NAME
            }
            Agent::Codex => OPENAI_API_KEY_ENV_NAME,
            Agent::Amp => AMP_API_KEY_ENV_NAME,
            Agent::Kimi => KIMI_API_KEY_ENV_NAME,
            Agent::Grok => XAI_API_KEY_ENV_NAME,
            Agent::Antigravity | Agent::Gemini => GEMINI_API_KEY_ENV_NAME,
            Agent::Cursor => CURSOR_API_KEY_ENV_NAME,
            Agent::Muse => META_API_KEY_ENV_NAME,
            Agent::Opencode | Agent::Omp | Agent::Hermes => match self.provider {
                AiProvider::Anthropic => ANTHROPIC_API_KEY_ENV_NAME,
                AiProvider::OpenAi => OPENAI_API_KEY_ENV_NAME,
                AiProvider::Xai => XAI_API_KEY_ENV_NAME,
                AiProvider::Moonshot => MOONSHOT_API_KEY_ENV_NAME,
                AiProvider::Zai => ZHIPU_API_KEY_ENV_NAME,
                AiProvider::Minimax => MINIMAX_API_KEY_ENV_NAME,
                AiProvider::Google => GEMINI_API_KEY_ENV_NAME,
                AiProvider::Cursor => CURSOR_API_KEY_ENV_NAME,
                AiProvider::Meta => META_API_KEY_ENV_NAME,
                AiProvider::OpenRouter => OPENROUTER_API_KEY_ENV_NAME,
                AiProvider::Amp | AiProvider::Opencode => OPENCODE_API_KEY_ENV_NAME,
            },
        }
    }

    pub fn resolved_credential_descriptor(
        &self,
        agent: Agent,
    ) -> ConfigResult<ResolvedCredentialDescriptor> {
        if !self.supports_agent(agent) {
            return Err(ConfigError::msg(format_args!(
                "account {:?} cannot authenticate {agent}",
                self.name
            )));
        }
        let (mode, env_name) = match &self.credential {
            AccountCredential::Profile { .. } => {
                return Err(ConfigError::msg(format_args!(
                    "account {:?} has no environment credential route",
                    self.name
                )));
            }
            AccountCredential::ApiKey { .. } => {
                (AuthForwardMode::ApiKey, self.api_key_variable(agent))
            }
            AccountCredential::OAuthToken { .. } => (
                AuthForwardMode::OAuthToken,
                CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
            ),
        };
        Ok(ResolvedCredentialDescriptor {
            agent,
            provider: self.provider,
            mode,
            env_name,
        })
    }

    const fn default_api_url(&self, agent: Agent) -> Option<&'static str> {
        match (agent, self.provider) {
            (Agent::Claude, AiProvider::Moonshot) => Some("https://api.kimi.com/coding"),
            (Agent::Claude, AiProvider::Zai) => Some("https://api.z.ai/api/anthropic"),
            (Agent::Claude, AiProvider::Minimax) => Some("https://api.minimax.io/anthropic"),
            (Agent::Codex, AiProvider::Moonshot) => Some("https://api.kimi.com/coding/v1"),
            (Agent::Codex, AiProvider::Zai) => Some("https://api.z.ai/api/v1"),
            (Agent::Codex, AiProvider::Minimax) => Some("https://api.minimax.io/v1"),
            (Agent::Opencode | Agent::Omp | Agent::Hermes, AiProvider::OpenRouter) => {
                Some("https://openrouter.ai/api/v1")
            }
            _ => None,
        }
    }

    fn is_valid_http_endpoint(url: &str) -> bool {
        (url.starts_with("https://") || url.starts_with("http://"))
            && !url.contains(char::is_whitespace)
    }

    fn validate(&self, id: &str) -> ConfigResult<()> {
        validate_account_id(id)?;
        if self.name.trim().is_empty() {
            return Err(ConfigError::msg(format_args!(
                "account {id:?} has an empty name"
            )));
        }
        match &self.credential {
            AccountCredential::Profile {
                agent,
                directory,
                xdg_roots,
                source_selector,
            } => {
                if !directory.is_absolute() || !self.compatible_agent(*agent) {
                    return Err(ConfigError::msg(format_args!(
                        "invalid profile account {id:?}"
                    )));
                }
                if source_selector.is_some() && !matches!(agent, Agent::Omp | Agent::Hermes) {
                    return Err(ConfigError::msg(format_args!(
                        "account {id:?} has a store selector for unsupported agent {agent}"
                    )));
                }
                if let Some(selector) = source_selector {
                    selector.validate(id)?;
                }
                if let Some(roots) = xdg_roots {
                    let supports_xdg_roots = matches!(
                        agent.runtime().state_paths().folder_env_var,
                        Some(var) if matches!(var.kind, jackin_core::FolderVarKind::XdgRoot)
                    );
                    if !supports_xdg_roots {
                        return Err(ConfigError::msg(format_args!(
                            "account {id:?} sets xdg_roots, but its agent does not use XDG roots"
                        )));
                    }
                    if !roots.data.is_absolute()
                        || !roots.config.is_absolute()
                        || !roots.cache.is_absolute()
                    {
                        return Err(ConfigError::msg(format_args!(
                            "account {id:?} requires absolute xdg_roots directories"
                        )));
                    }
                }
            }
            AccountCredential::ApiKey { value, .. }
            | AccountCredential::OAuthToken { value, .. } => {
                if value.as_persisted_str().trim().is_empty() || value.is_on_demand() {
                    return Err(ConfigError::msg(format_args!(
                        "account {id:?} requires a nonempty launch-time credential"
                    )));
                }
            }
        }
        if let AccountCredential::ApiKey {
            base_url, model, ..
        } = &self.credential
        {
            if model
                .as_deref()
                .is_some_and(|model| model.trim().is_empty())
            {
                return Err(ConfigError::msg(format_args!(
                    "account {id:?} has an empty model"
                )));
            }
            if base_url
                .as_deref()
                .is_some_and(|url| !Self::is_valid_http_endpoint(url))
            {
                return Err(ConfigError::msg(format_args!(
                    "account {id:?} requires an HTTP(S) endpoint"
                )));
            }
        }
        if !Agent::ALL.iter().any(|a| self.compatible_agent(*a)) {
            return Err(ConfigError::msg(format_args!(
                "account {id:?} has no compatible agent"
            )));
        }
        Ok(())
    }

    pub const fn auth_mode(&self) -> AuthForwardMode {
        match self.credential {
            AccountCredential::Profile { .. } => AuthForwardMode::Sync,
            AccountCredential::ApiKey { .. } => AuthForwardMode::ApiKey,
            AccountCredential::OAuthToken { .. } => AuthForwardMode::OAuthToken,
        }
    }
    pub fn source_directory(&self) -> Option<&Path> {
        match &self.credential {
            AccountCredential::Profile { directory, .. } => Some(directory),
            _ => None,
        }
    }
    pub fn credential_env(&self, agent: Agent) -> ConfigResult<BTreeMap<String, EnvValue>> {
        self.credential_env_for_endpoint(agent, None)
    }

    pub fn credential_env_for_instance(
        &self,
        agent: Agent,
        base_url: Option<&str>,
    ) -> ConfigResult<BTreeMap<String, EnvValue>> {
        self.credential_env_for_endpoint(agent, base_url)
    }

    fn credential_env_for_endpoint(
        &self,
        agent: Agent,
        endpoint_override: Option<&str>,
    ) -> ConfigResult<BTreeMap<String, EnvValue>> {
        if !self.supports_agent(agent) {
            return Err(ConfigError::msg(format_args!(
                "account {:?} cannot authenticate {agent}",
                self.name
            )));
        }
        let route = match &self.credential {
            AccountCredential::Profile { .. } => None,
            AccountCredential::ApiKey { .. } | AccountCredential::OAuthToken { .. } => {
                Some(self.resolved_credential_descriptor(agent)?)
            }
        };
        let account_endpoint = match &self.credential {
            AccountCredential::ApiKey { base_url, .. } => base_url.as_deref(),
            AccountCredential::Profile { .. } | AccountCredential::OAuthToken { .. } => None,
        };
        let endpoint = endpoint_override.or(account_endpoint);
        if endpoint.is_some_and(|url| !Self::is_valid_http_endpoint(url)) {
            return Err(ConfigError::msg(format_args!(
                "account {:?} requires an HTTP(S) endpoint",
                self.name
            )));
        }
        let mut env = BTreeMap::new();
        match &self.credential {
            AccountCredential::Profile { .. } => {}
            AccountCredential::OAuthToken { value, .. } => {
                let route = route.ok_or_else(|| {
                    ConfigError::msg(format_args!(
                        "account {:?} has no environment credential route",
                        self.name
                    ))
                })?;
                env.insert(route.env_name.into(), value.clone());
                if let Some(url) = endpoint {
                    env.insert("ANTHROPIC_BASE_URL".into(), EnvValue::from(url));
                }
            }
            AccountCredential::ApiKey {
                value,
                base_url: _,
                model,
            } => {
                if matches!(
                    agent,
                    Agent::Claude | Agent::Codex | Agent::Opencode | Agent::Omp | Agent::Hermes
                ) && Some(self.provider) != AiProvider::for_agent(agent)
                    && model.as_deref().is_none_or(|model| model.trim().is_empty())
                {
                    return Err(ConfigError::msg(format_args!(
                        "account {:?} requires an explicit model for {agent}",
                        self.name
                    )));
                }
                let route = route.ok_or_else(|| {
                    ConfigError::msg(format_args!(
                        "account {:?} has no environment credential route",
                        self.name
                    ))
                })?;
                env.insert(route.env_name.into(), value.clone());
                if agent == Agent::Claude
                    && let Some(model) = model
                {
                    for name in [
                        "ANTHROPIC_MODEL",
                        "ANTHROPIC_DEFAULT_OPUS_MODEL",
                        "ANTHROPIC_DEFAULT_SONNET_MODEL",
                        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                    ] {
                        env.insert(name.into(), EnvValue::from(model.as_str()));
                    }
                }
                let default_url = self.default_api_url(agent);
                if matches!(agent, Agent::Omp | Agent::Hermes) && endpoint.is_some() {
                    return Err(ConfigError::msg(format_args!(
                        "account {:?} has an endpoint override for {agent}, but that provider configuration is unsupported",
                        self.name
                    )));
                }
                if !matches!(agent, Agent::Opencode | Agent::Omp | Agent::Hermes)
                    && let Some(url) = endpoint.or(default_url)
                {
                    let name = match agent {
                        Agent::Claude => "ANTHROPIC_BASE_URL",
                        Agent::Codex => "OPENAI_BASE_URL",
                        Agent::Kimi => "KIMI_BASE_URL",
                        _ => {
                            return Err(ConfigError::msg(format_args!(
                                "endpoint overrides are unsupported for this agent"
                            )));
                        }
                    };
                    env.insert(name.into(), EnvValue::from(url));
                }
            }
        }
        Ok(env)
    }
}

pub(crate) fn account_source_fingerprint(account: &AccountConfig) -> String {
    let mut digest = Sha256::new();
    hash_component(&mut digest, account.provider.slug());
    match &account.credential {
        AccountCredential::Profile {
            agent,
            directory,
            xdg_roots,
            source_selector,
        } => {
            hash_component(&mut digest, "profile");
            hash_component(&mut digest, agent.slug());
            hash_path_component(&mut digest, directory);
            if let Some(roots) = xdg_roots {
                hash_component(&mut digest, "xdg_roots");
                hash_path_component(&mut digest, &roots.data);
                hash_path_component(&mut digest, &roots.config);
                hash_path_component(&mut digest, &roots.cache);
            } else {
                hash_component(&mut digest, "no_xdg_roots");
            }
            if let Some(selector) = source_selector {
                hash_component(&mut digest, "source_selector");
                hash_component(&mut digest, &selector.entry);
                if let Some(profile) = &selector.profile {
                    hash_component(&mut digest, "profile");
                    hash_component(&mut digest, profile);
                } else {
                    hash_component(&mut digest, "no_profile");
                }
            } else {
                hash_component(&mut digest, "no_source_selector");
            }
        }
        AccountCredential::ApiKey {
            value, base_url, ..
        } => {
            hash_component(&mut digest, "api_key");
            hash_env_value(&mut digest, value);
            hash_optional_component(&mut digest, base_url.as_deref());
        }
        AccountCredential::OAuthToken { agent, value } => {
            hash_component(&mut digest, "oauth_token");
            hash_component(&mut digest, agent.slug());
            hash_env_value(&mut digest, value);
        }
    }
    hex::encode(digest.finalize())
}

fn hash_path_component(digest: &mut Sha256, path: &Path) {
    let canonical = crate::paths::canonical_path_identity(path);
    hash_component(digest, canonical.to_string_lossy().as_ref());
}

fn hash_component(digest: &mut Sha256, value: &str) {
    digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    digest.update(value.as_bytes());
}

fn hash_optional_component(digest: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash_component(digest, "some");
            hash_component(digest, value);
        }
        None => hash_component(digest, "none"),
    }
}

fn hash_env_value(digest: &mut Sha256, value: &EnvValue) {
    match value {
        EnvValue::Plain(value) => {
            hash_component(digest, "plain");
            hash_component(digest, value);
        }
        EnvValue::Extended(value) => {
            hash_component(digest, "extended");
            hash_component(digest, &value.value);
            hash_component(digest, if value.on_demand { "true" } else { "false" });
        }
        EnvValue::OpRef(value) => {
            hash_component(digest, "op_ref");
            hash_component(digest, &value.op);
            hash_component(
                digest,
                &jackin_core::display_op_breadcrumb_path(&value.path),
            );
            hash_optional_component(digest, value.account.as_deref());
            hash_component(digest, if value.on_demand { "true" } else { "false" });
        }
    }
}
pub fn validate_account_id(id: &str) -> ConfigResult<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        || !id
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        return Err(ConfigError::msg(format_args!(
            "account ID must be a lowercase slug of 1–64 characters"
        )));
    }
    Ok(())
}
pub fn resolve_account<'a>(
    cfg: &'a AppConfig,
    agent: Agent,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> ConfigResult<Option<&'a AccountConfig>> {
    let ws = workspace
        .map(|name| {
            cfg.workspaces
                .get(name.as_str())
                .ok_or_else(|| ConfigError::WorkspaceNotFound(name.as_str().into()))
        })
        .transpose()?;
    let binding = ws
        .and_then(|w| w.roles.get(role))
        .and_then(|r| r.account_bindings.get(&agent))
        .or_else(|| ws.and_then(|w| w.account_bindings.get(&agent)))
        .or_else(|| cfg.account_bindings.get(&agent));
    if let Some(id) = binding {
        if ws.is_some_and(|w| !w.accounts.contains(id)) {
            return Err(ConfigError::msg(format_args!(
                "account {id:?} is not assigned to this workspace"
            )));
        }
        let account = cfg
            .accounts
            .get(id)
            .ok_or_else(|| ConfigError::msg(format_args!("unknown account {id:?}")))?;
        if !account.supports_agent(agent) {
            return Err(ConfigError::msg(format_args!(
                "account {id:?} does not support {agent}"
            )));
        }
        return Ok(Some(account));
    }
    let Some(ws) = ws else { return Ok(None) };
    for id in &ws.accounts {
        if !cfg.accounts.contains_key(id) {
            return Err(ConfigError::msg(format_args!("unknown account {id:?}")));
        }
    }
    let mut candidates = ws
        .accounts
        .iter()
        .filter_map(|id| cfg.accounts.get(id))
        .filter(|a| a.supports_agent(agent));
    let selected = candidates.next();
    if candidates.next().is_some() {
        return Err(ConfigError::msg(format_args!(
            "multiple accounts support {agent}; select an account binding"
        )));
    }
    Ok(selected)
}
pub const BOOTSTRAP_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapState {
    pub version: u32,
    #[serde(default)]
    pub fresh_install: bool,
}

impl BootstrapState {
    pub const fn initialized() -> Self {
        Self {
            version: BOOTSTRAP_VERSION,
            fresh_install: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfiguration {
    pub agent: Agent,
    pub account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoked_via_wrapper: Option<WrapperSpec>,
}

impl AgentConfiguration {
    pub fn validate(
        &self,
        id: &str,
        accounts: &BTreeMap<String, AccountConfig>,
    ) -> ConfigResult<()> {
        validate_account_id(id).map_err(|_| {
            ConfigError::msg(format_args!(
                "configuration ID must be a lowercase slug of 1–64 characters"
            ))
        })?;
        let account = accounts
            .get(&self.account)
            .ok_or_else(|| ConfigError::msg(format_args!("unknown account {:?}", self.account)))?;
        if !account.supports_agent(self.agent) {
            return Err(ConfigError::msg(format_args!(
                "account {:?} is not authorized for {}",
                self.account, self.agent
            )));
        }
        if self
            .model
            .as_deref()
            .is_some_and(|model| model.trim().is_empty())
        {
            return Err(ConfigError::msg(format_args!(
                "configuration {id:?} has an empty model"
            )));
        }
        if self
            .base_url
            .as_deref()
            .is_some_and(|url| !AccountConfig::is_valid_http_endpoint(url))
        {
            return Err(ConfigError::msg(format_args!(
                "configuration {id:?} requires an HTTP(S) endpoint"
            )));
        }
        if self.base_url.is_some() && matches!(self.agent, Agent::Omp | Agent::Hermes) {
            return Err(ConfigError::msg(format_args!(
                "configuration {id:?} has an endpoint override for {}, but that provider configuration is unsupported",
                self.agent
            )));
        }
        if self
            .display_label
            .as_deref()
            .is_some_and(|label| label.trim().is_empty())
        {
            return Err(ConfigError::msg(format_args!(
                "configuration {id:?} has an empty display label"
            )));
        }
        self.validate_launch_transport(id)?;
        Ok(())
    }

    fn validate_launch_transport(&self, id: &str) -> ConfigResult<()> {
        if self.invoked_via_wrapper.is_some() {
            return Err(ConfigError::msg(format_args!(
                "configuration {id:?} declares an unsupported shell wrapper; jackin capsule launches cannot execute arbitrary host wrappers safely"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInstance {
    pub config_id: String,
    pub agent: Agent,
    pub account_id: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub xdg_roots: Option<XdgRoots>,
    pub label: String,
    pub synthesized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedLaunchInstance<'a> {
    pub config_id: &'a str,
    pub agent: Agent,
    pub account_id: &'a str,
}

impl ResolvedInstance {
    fn bind(config_id: &str, config: &AgentConfiguration, account: &AccountConfig) -> Self {
        let (account_model, account_url) = match &account.credential {
            AccountCredential::ApiKey {
                base_url, model, ..
            } => (model.clone(), base_url.clone()),
            _ => (None, None),
        };
        let label = config
            .display_label
            .clone()
            .unwrap_or_else(|| format!("{} · {}", config.agent.label(), account.name));
        let xdg_roots = match &account.credential {
            AccountCredential::Profile {
                agent, xdg_roots, ..
            } if *agent == config.agent => xdg_roots.clone(),
            _ => None,
        };
        Self {
            config_id: config_id.to_owned(),
            agent: config.agent,
            account_id: config.account.clone(),
            model: config.model.clone().or(account_model),
            base_url: config.base_url.clone().or(account_url),
            xdg_roots,
            label,
            synthesized: false,
        }
    }

    fn synthesize(account_id: &str, agent: Agent, account: &AccountConfig) -> Self {
        let (model, base_url) = match &account.credential {
            AccountCredential::ApiKey {
                base_url, model, ..
            } => (model.clone(), base_url.clone()),
            _ => (None, None),
        };
        let xdg_roots = match &account.credential {
            AccountCredential::Profile {
                agent: owner,
                xdg_roots,
                ..
            } if *owner == agent => xdg_roots.clone(),
            _ => None,
        };
        Self {
            config_id: format!("{account_id}@{}", agent.slug()),
            agent,
            account_id: account_id.to_owned(),
            model,
            base_url,
            xdg_roots,
            label: format!("{} · {}", agent.label(), account.name),
            synthesized: true,
        }
    }
}

pub fn resolve_launch(
    cfg: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
    one_launch: Option<&[String]>,
    agent: Option<Agent>,
) -> ConfigResult<Vec<ResolvedInstance>> {
    let ws = workspace
        .map(|name| {
            cfg.workspaces
                .get(name.as_str())
                .ok_or_else(|| ConfigError::WorkspaceNotFound(name.as_str().into()))
        })
        .transpose()?;
    let authorized =
        |account_id: &str| ws.is_none_or(|w| w.accounts.iter().any(|id| id == account_id));

    if let Some(selection) = one_launch {
        let mut seen = BTreeSet::new();
        let mut instances = Vec::with_capacity(selection.len());
        for id in selection {
            if !seen.insert(id) {
                return Err(ConfigError::msg(format_args!(
                    "duplicate configuration {id:?} in launch selection"
                )));
            }
            instances.push(bind_explicit(cfg, ws, id)?);
        }
        return checked_launch_instances(instances);
    }

    let inherited = ws
        .and_then(|w| w.roles.get(role))
        .and_then(|r| r.default_launch.as_deref())
        .or(ws.and_then(|w| w.default_launch.as_deref()))
        .or(cfg.default_launch.as_deref());
    if let Some(ids) = inherited {
        let scope_is_global = ws
            .and_then(|w| w.roles.get(role))
            .and_then(|r| r.default_launch.as_deref())
            .is_none()
            && ws.and_then(|w| w.default_launch.as_deref()).is_none();
        let mut instances = Vec::with_capacity(ids.len());
        for id in ids {
            let config = cfg.agent_configurations.get(id).ok_or_else(|| {
                ConfigError::msg(format_args!("unknown agent configuration {id:?}"))
            })?;
            if scope_is_global && !authorized(&config.account) {
                continue;
            }
            instances.push(bind_explicit(cfg, ws, id)?);
        }
        return checked_launch_instances(instances);
    }

    if let Some(committed) = agent
        && let Some(selected) = resolve_account(cfg, committed, workspace, role)?
        && let Some(id) = cfg
            .accounts
            .iter()
            .find(|(_, account)| std::ptr::eq(*account, selected))
            .map(|(id, _)| id.clone())
    {
        return checked_launch_instances(vec![ResolvedInstance::synthesize(
            &id, committed, selected,
        )]);
    }
    let agents: &[Agent] = match agent.as_slice() {
        [committed] => std::slice::from_ref(committed),
        _ => Agent::ALL,
    };
    let mut eligible = Vec::new();
    match ws {
        Some(w) => {
            for id in &w.accounts {
                let account = cfg
                    .accounts
                    .get(id)
                    .ok_or_else(|| ConfigError::msg(format_args!("unknown account {id:?}")))?;
                for agent in agents {
                    if account.supports_agent(*agent) {
                        eligible.push(ResolvedInstance::synthesize(id, *agent, account));
                    }
                }
            }
        }
        None => {
            for (id, account) in &cfg.accounts {
                for agent in agents {
                    if account.supports_agent(*agent) {
                        eligible.push(ResolvedInstance::synthesize(id, *agent, account));
                    }
                }
            }
        }
    }
    if eligible.is_empty() {
        return Err(ConfigError::msg(format_args!(
            "no eligible account for this launch"
        )));
    }
    if eligible.len() > 1 {
        return Err(ConfigError::msg(format_args!(
            "multiple accounts are eligible; select launch configurations"
        )));
    }
    Ok(eligible)
}

pub fn resolve_saved_launch(
    cfg: &AppConfig,
    workspace: Option<&WorkspaceName>,
    saved: &[SavedLaunchInstance<'_>],
) -> ConfigResult<Vec<ResolvedInstance>> {
    let ws = workspace
        .map(|name| {
            cfg.workspaces
                .get(name.as_str())
                .ok_or_else(|| ConfigError::WorkspaceNotFound(name.as_str().into()))
        })
        .transpose()?;
    if saved.is_empty() {
        return Err(ConfigError::msg(format_args!(
            "saved launch selection is empty"
        )));
    }

    let mut seen = BTreeSet::new();
    let mut instances = Vec::with_capacity(saved.len());
    for tuple in saved {
        if !seen.insert(tuple.config_id) {
            return Err(ConfigError::msg(format_args!(
                "duplicate configuration {:?} in saved launch selection",
                tuple.config_id
            )));
        }

        let instance = if cfg.agent_configurations.contains_key(tuple.config_id) {
            let instance = bind_explicit(cfg, ws, tuple.config_id)?;
            if instance.agent != tuple.agent || instance.account_id != tuple.account_id {
                return Err(ConfigError::msg(format_args!(
                    "saved configuration {:?} no longer matches its recorded agent/account tuple",
                    tuple.config_id
                )));
            }
            instance
        } else {
            let expected_id = format!("{}@{}", tuple.account_id, tuple.agent.slug());
            if tuple.config_id != expected_id.as_str() {
                return Err(ConfigError::msg(format_args!(
                    "saved synthesized configuration {:?} does not match its recorded account/agent tuple",
                    tuple.config_id
                )));
            }
            if let Some(workspace) = ws
                && !workspace.accounts.iter().any(|id| id == tuple.account_id)
            {
                return Err(ConfigError::msg(format_args!(
                    "account {:?} is not assigned to this workspace",
                    tuple.account_id
                )));
            }
            let account = cfg.accounts.get(tuple.account_id).ok_or_else(|| {
                ConfigError::msg(format_args!("unknown account {:?}", tuple.account_id))
            })?;
            if !account.supports_agent(tuple.agent) {
                return Err(ConfigError::msg(format_args!(
                    "account {:?} is not authorized for {}",
                    tuple.account_id, tuple.agent
                )));
            }
            ResolvedInstance::synthesize(tuple.account_id, tuple.agent, account)
        };
        instances.push(instance);
    }

    checked_launch_instances(instances)
}

fn checked_launch_instances(
    instances: Vec<ResolvedInstance>,
) -> ConfigResult<Vec<ResolvedInstance>> {
    use jackin_core::FolderVarKind;
    for agent in Agent::ALL {
        let count = instances
            .iter()
            .filter(|instance| instance.agent == *agent)
            .count();
        if count < 2 {
            continue;
        }
        match agent
            .runtime()
            .state_paths()
            .folder_env_var
            .map(|var| (var.name, var.kind))
        {
            Some((_, FolderVarKind::Dir | FolderVarKind::Parent)) => {}
            Some((name, FolderVarKind::XdgRoot)) => {
                return Err(ConfigError::msg(format_args!(
                    "agent {agent} admits only one account per container: \
                     isolating several accounts needs {name}, which would also \
                     redirect unrelated XDG consumers of the pane process"
                )));
            }
            None => {
                return Err(ConfigError::msg(format_args!(
                    "agent {agent} admits only one account per container: \
                     it has no config-folder env var to isolate instances"
                )));
            }
        }
    }
    Ok(instances)
}

fn bind_explicit(
    cfg: &AppConfig,
    ws: Option<&WorkspaceConfig>,
    id: &str,
) -> ConfigResult<ResolvedInstance> {
    let config = cfg
        .agent_configurations
        .get(id)
        .ok_or_else(|| ConfigError::msg(format_args!("unknown agent configuration {id:?}")))?;
    config.validate_launch_transport(id)?;
    if let Some(w) = ws
        && !w.accounts.contains(&config.account)
    {
        return Err(ConfigError::msg(format_args!(
            "account {:?} is not assigned to this workspace",
            config.account
        )));
    }
    let account = cfg
        .accounts
        .get(&config.account)
        .ok_or_else(|| ConfigError::msg(format_args!("unknown account {:?}", config.account)))?;
    if !account.supports_agent(config.agent) {
        return Err(ConfigError::msg(format_args!(
            "account {:?} is not authorized for {}",
            config.account, config.agent
        )));
    }
    Ok(ResolvedInstance::bind(id, config, account))
}
impl AppConfig {
    pub fn credential_descriptors_for_account(
        &self,
        account_id: &str,
    ) -> ConfigResult<Vec<ResolvedCredentialDescriptor>> {
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| ConfigError::msg(format_args!("unknown account {account_id:?}")))?;
        if matches!(&account.credential, AccountCredential::Profile { .. }) {
            return Err(ConfigError::msg(format_args!(
                "account {account_id:?} has no environment credential route"
            )));
        }
        let mut agents = BTreeSet::new();
        for configuration in self.agent_configurations.values() {
            if configuration.account == account_id {
                agents.insert(configuration.agent);
            }
        }
        let mut collect_binding = |bindings: &BTreeMap<Agent, String>| {
            for (agent, selected) in bindings {
                if selected == account_id {
                    agents.insert(*agent);
                }
            }
        };
        collect_binding(&self.account_bindings);
        for workspace in self.workspaces.values() {
            if workspace.accounts.iter().any(|id| id == account_id) {
                collect_binding(&workspace.account_bindings);
                for role in workspace.roles.values() {
                    collect_binding(&role.account_bindings);
                }
            }
        }
        agents.extend(
            Agent::ALL
                .iter()
                .copied()
                .filter(|agent| account.supports_agent(*agent)),
        );
        let descriptors = Agent::ALL
            .iter()
            .copied()
            .filter(|agent| agents.contains(agent) && account.supports_agent(*agent))
            .filter_map(|agent| account.resolved_credential_descriptor(agent).ok())
            .collect::<Vec<_>>();
        if descriptors.is_empty() {
            return Err(ConfigError::msg(format_args!(
                "account {account_id:?} has no compatible launch route"
            )));
        }
        Ok(descriptors)
    }

    pub fn validate_accounts(&self) -> ConfigResult<()> {
        for (id, account) in &self.accounts {
            account.validate(id)?;
        }
        for (id, config) in &self.agent_configurations {
            config.validate(id, &self.accounts)?;
        }
        self.check_account_bindings(&self.account_bindings, None)?;
        self.validate_launch_list(self.default_launch.as_deref(), None)?;
        for ws in self.workspaces.values() {
            let mut seen = BTreeSet::new();
            for id in &ws.accounts {
                if !self.accounts.contains_key(id) || !seen.insert(id) {
                    return Err(ConfigError::msg(format_args!(
                        "unknown or duplicate workspace account {id:?}"
                    )));
                }
            }
            self.check_account_bindings(&ws.account_bindings, Some(&ws.accounts))?;
            self.validate_launch_list(ws.default_launch.as_deref(), Some(&ws.accounts))?;
            for role in ws.roles.values() {
                self.check_account_bindings(&role.account_bindings, Some(&ws.accounts))?;
                self.validate_launch_list(role.default_launch.as_deref(), Some(&ws.accounts))?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate_registry_and_bindings(&self) -> ConfigResult<()> {
        for (id, account) in &self.accounts {
            account.validate(id)?;
        }
        self.check_account_bindings(&self.account_bindings, None)?;
        for ws in self.workspaces.values() {
            let mut seen = BTreeSet::new();
            for id in &ws.accounts {
                if !self.accounts.contains_key(id) || !seen.insert(id) {
                    return Err(ConfigError::msg(format_args!(
                        "unknown or duplicate workspace account {id:?}"
                    )));
                }
            }
            self.check_account_bindings(&ws.account_bindings, Some(&ws.accounts))?;
            for role in ws.roles.values() {
                self.check_account_bindings(&role.account_bindings, Some(&ws.accounts))?;
            }
        }
        Ok(())
    }

    fn check_account_bindings(
        &self,
        bindings: &BTreeMap<Agent, String>,
        allowed: Option<&Vec<String>>,
    ) -> ConfigResult<()> {
        for (agent, id) in bindings {
            let account = self
                .accounts
                .get(id)
                .ok_or_else(|| ConfigError::msg(format_args!("unknown account {id:?}")))?;
            if !account.supports_agent(*agent) || allowed.is_some_and(|ids| !ids.contains(id)) {
                return Err(ConfigError::msg(format_args!(
                    "account {id:?} is not authorized for {agent}"
                )));
            }
        }
        Ok(())
    }

    fn validate_launch_list(
        &self,
        ids: Option<&[String]>,
        allowed: Option<&Vec<String>>,
    ) -> ConfigResult<()> {
        let Some(ids) = ids else { return Ok(()) };
        let mut seen = BTreeSet::new();
        for id in ids {
            if !seen.insert(id) {
                return Err(ConfigError::msg(format_args!(
                    "duplicate launch configuration {id:?}"
                )));
            }
            let config = self.agent_configurations.get(id).ok_or_else(|| {
                ConfigError::msg(format_args!("unknown agent configuration {id:?}"))
            })?;
            if allowed.is_some_and(|ids| !ids.contains(&config.account)) {
                return Err(ConfigError::msg(format_args!(
                    "account {:?} is not assigned to this workspace",
                    config.account
                )));
            }
        }
        Ok(())
    }

    pub fn prune_account_bindings(&mut self, id: &str) {
        self.account_bindings.retain(|_, selected| selected != id);
        for ws in self.workspaces.values_mut() {
            ws.account_bindings.retain(|_, selected| selected != id);
            for role in ws.roles.values_mut() {
                role.account_bindings.retain(|_, selected| selected != id);
            }
        }
    }

    pub fn prune_agent_configurations(&mut self, account_id: &str) {
        let removed: Vec<String> = self
            .agent_configurations
            .iter()
            .filter(|(_, config)| config.account == account_id)
            .map(|(id, _)| id.clone())
            .collect();
        if removed.is_empty() {
            return;
        }
        for id in &removed {
            self.agent_configurations.remove(id);
        }
        let scrub = |list: &mut Option<Vec<String>>| {
            if let Some(ids) = list {
                ids.retain(|id| !removed.contains(id));
            }
        };
        scrub(&mut self.default_launch);
        for ws in self.workspaces.values_mut() {
            scrub(&mut ws.default_launch);
            for role in ws.roles.values_mut() {
                scrub(&mut role.default_launch);
            }
        }
    }
}

#[cfg(test)]
mod tests;
