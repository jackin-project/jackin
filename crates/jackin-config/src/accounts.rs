// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Named credentials and workspace account authorization.

use crate::schema::WorkspaceConfig;
use crate::{AppConfig, ConfigError, ConfigResult};
use jackin_core::{Agent, AuthForwardMode, EnvValue, WorkspaceName};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(crate) mod discovery;
pub(crate) mod stores;
pub(crate) mod zshrc;

/// Service issuing an account's credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    /// Anthropic API and Claude subscriptions.
    Anthropic,
    /// `OpenAI` API and `ChatGPT` subscriptions.
    #[serde(rename = "openai")]
    OpenAi,
    /// Sourcegraph Amp.
    Amp,
    /// xAI.
    Xai,
    /// `OpenCode` Zen.
    Opencode,
    /// Moonshot Kimi.
    Moonshot,
    /// Z.ai.
    Zai,
    /// `MiniMax`.
    Minimax,
    /// Google AI Studio / Gemini API (Antigravity + Gemini CLI).
    Google,
    /// Cursor API.
    Cursor,
    /// Meta API (Muse).
    Meta,
    /// `OpenRouter` (multi-provider clients only; no native agent).
    #[serde(rename = "openrouter")]
    OpenRouter,
}
impl AiProvider {
    /// Every variant in declaration order. Iteration sites consult this
    /// instead of hand-rolling their own array.
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

    /// Canonical provider identifier.
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
    /// Native billing service for an agent, if it has one.
    ///
    /// Returns `None` for pure multi-provider clients (Omp, Hermes),
    /// which route arbitrary providers and have no native billing.
    /// Callers comparing against an account's provider must compare
    /// `Some(provider) == for_agent(agent)` so `None` never matches.
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
            _ => Err(ConfigError::msg(format!("unknown AI provider {value:?}"))),
        }
    }
}

/// Immutable selector for one entry in a multi-provider profile store.
///
/// `entry` is the store's provider/account key. `profile` is the optional
/// profile label used by stores that distinguish profiles. The selector is
/// persisted with the account so launch never falls back to whichever entry
/// happens to enumerate first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSelector {
    /// Exact provider/account key in the source store.
    pub entry: String,
    /// Exact profile label, when the source store exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

impl ProfileSelector {
    fn validate(&self, id: &str) -> ConfigResult<()> {
        if self.entry.trim().is_empty() || self.entry.contains('\0') {
            return Err(ConfigError::msg(format!(
                "account {id:?} has an invalid profile-store entry selector"
            )));
        }
        if self
            .profile
            .as_deref()
            .is_some_and(|profile| profile.trim().is_empty() || profile.contains('\0'))
        {
            return Err(ConfigError::msg(format!(
                "account {id:?} has an invalid profile-store profile selector"
            )));
        }
        Ok(())
    }
}

/// Credential source. Secret values are redacted from Debug output.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountCredential {
    /// Agent-managed login stored in a selected configuration directory.
    Profile {
        /// Agent owning this profile's storage format.
        agent: Agent,
        /// Exact host configuration directory.
        directory: PathBuf,
        /// Explicit XDG roots for clients that split state across
        /// data/config/cache homes (Amp, `OpenCode`, and future XDG-root
        /// clients). Absolute directories; validated.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        xdg_roots: Option<XdgRoots>,
        /// Immutable entry/profile identity for Omp and Hermes stores.
        /// Missing selectors remain valid only for a source that proves it
        /// contains exactly one account at launch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_selector: Option<ProfileSelector>,
    },
    /// Provider API key, literal or an environment/1Password reference.
    ApiKey {
        /// Secret reference or literal.
        value: EnvValue,
        /// Optional provider endpoint override.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_url: Option<String>,
        /// Explicit provider model identifier.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Agent subscription token.
    OAuthToken {
        /// Agent accepting this token.
        agent: Agent,
        /// Secret reference or literal.
        value: EnvValue,
    },
}
/// Explicit XDG state roots for one profile credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XdgRoots {
    /// Data home (`XDG_DATA_HOME` equivalent).
    pub data: PathBuf,
    /// Config home (`XDG_CONFIG_HOME` equivalent).
    pub config: PathBuf,
    /// Cache home (`XDG_CACHE_HOME` equivalent).
    pub cache: PathBuf,
}

/// Shell-wrapper invocation found while importing shell configuration.
///
/// The current capsule launch protocol deliberately does not execute arbitrary
/// host shell functions or helper commands. A configuration carrying this
/// provenance is therefore rejected before a [`ResolvedInstance`] is created;
/// its identity and arguments are never transported into the capsule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrapperSpec {
    /// Wrapper identity: shell function or helper command name.
    pub identity: String,
    /// Arguments passed to the wrapper at the call site, in order.
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
/// Reusable named account. Workspaces explicitly authorize account IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountConfig {
    /// Whether this account may authenticate launches.
    #[serde(
        default = "account_enabled_by_default",
        skip_serializing_if = "crate::schema::bool_matches::<true>"
    )]
    pub enabled: bool,
    /// Human-readable account name.
    pub name: String,
    /// Credential issuer.
    pub provider: AiProvider,
    /// Credential source.
    pub credential: AccountCredential,
}
const fn account_enabled_by_default() -> bool {
    true
}
impl AccountConfig {
    /// Whether this credential can authenticate this agent.
    pub fn supports_agent(&self, agent: Agent) -> bool {
        self.enabled && self.compatible_agent(agent)
    }

    fn compatible_agent(&self, agent: Agent) -> bool {
        match &self.credential {
            // Profile: the store owner must be the agent, and either the
            // provider is that agent's native billing or the agent is a
            // multi-provider client whose store holds arbitrary providers
            // (OpenCode, Omp, Hermes).
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
                        // Claude/Codex route a fixed set of Anthropic/OpenAI
                        // -compatible third parties. OpenRouter is
                        // deliberately NOT routed here: it reaches
                        // Claude/Codex-shaped workloads only through
                        // OpenCode/Omp/Hermes with an explicit model.
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
                        // Multi-provider clients accept every provider
                        // except Amp, whose key has no third-party use.
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
                "ANTHROPIC_AUTH_TOKEN"
            }
            Agent::Claude => "ANTHROPIC_API_KEY",
            Agent::Codex if matches!(self.provider, AiProvider::Moonshot) => "KIMI_API_KEY",
            Agent::Codex if matches!(self.provider, AiProvider::Minimax) => "MINIMAX_API_KEY",
            Agent::Codex => "OPENAI_API_KEY",
            Agent::Amp => "AMP_API_KEY",
            Agent::Kimi => "KIMI_API_KEY",
            Agent::Grok => "XAI_API_KEY",
            // Single-provider newcomers only accept their native provider,
            // so the variable is fixed per agent.
            Agent::Antigravity | Agent::Gemini => "GEMINI_API_KEY",
            Agent::Cursor => "CURSOR_API_KEY",
            Agent::Muse => "META_API_KEY",
            // Multi-provider clients select the variable per provider.
            // Provider-native names are used so the routed CLI finds the
            // key without extra mapping; the OpenCode Zen key is the
            // fallback for the two providers with no third-party variable
            // (Amp is unreachable here — excluded by compatibility —
            // and Opencode's own Zen key).
            Agent::Opencode | Agent::Omp | Agent::Hermes => match self.provider {
                AiProvider::Anthropic => "ANTHROPIC_API_KEY",
                AiProvider::OpenAi => "OPENAI_API_KEY",
                AiProvider::Xai => "XAI_API_KEY",
                AiProvider::Moonshot => "MOONSHOT_API_KEY",
                AiProvider::Zai => "ZHIPU_API_KEY",
                AiProvider::Minimax => "MINIMAX_API_KEY",
                AiProvider::Google => "GEMINI_API_KEY",
                AiProvider::Cursor => "CURSOR_API_KEY",
                AiProvider::Meta => "META_API_KEY",
                AiProvider::OpenRouter => "OPENROUTER_API_KEY",
                AiProvider::Amp | AiProvider::Opencode => "OPENCODE_API_KEY",
            },
        }
    }

    const fn default_api_url(&self, agent: Agent) -> Option<&'static str> {
        match (agent, self.provider) {
            (Agent::Claude, AiProvider::Moonshot) => Some("https://api.kimi.com/coding"),
            (Agent::Claude, AiProvider::Zai) => Some("https://api.z.ai/api/anthropic"),
            (Agent::Claude, AiProvider::Minimax) => Some("https://api.minimax.io/anthropic"),
            (Agent::Codex, AiProvider::Moonshot) => Some("https://api.kimi.com/coding/v1"),
            (Agent::Codex, AiProvider::Zai) => Some("https://api.z.ai/api/v1"),
            (Agent::Codex, AiProvider::Minimax) => Some("https://api.minimax.io/v1"),
            // Multi-provider clients route OpenRouter at its documented
            // base. Endpoint *application* for Omp/Hermes is deferred to
            // the provider-config lane (credential_env skips env
            // injection for them, like OpenCode); the URL is recorded
            // here so that lane has one source of truth.
            (Agent::Opencode | Agent::Omp | Agent::Hermes, AiProvider::OpenRouter) => {
                Some("https://openrouter.ai/api/v1")
            }
            // Native Google/Cursor/Meta keys need no endpoint override;
            // unknown third-party combinations fail closed (None) rather
            // than guessing a base URL.
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
            return Err(ConfigError::msg(format!(
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
                    return Err(ConfigError::msg(format!("invalid profile account {id:?}")));
                }
                if source_selector.is_some() && !matches!(agent, Agent::Omp | Agent::Hermes) {
                    return Err(ConfigError::msg(format!(
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
                        return Err(ConfigError::msg(format!(
                            "account {id:?} sets xdg_roots, but its agent does not use XDG roots"
                        )));
                    }
                    if !roots.data.is_absolute()
                        || !roots.config.is_absolute()
                        || !roots.cache.is_absolute()
                    {
                        return Err(ConfigError::msg(format!(
                            "account {id:?} requires absolute xdg_roots directories"
                        )));
                    }
                }
            }
            AccountCredential::ApiKey { value, .. }
            | AccountCredential::OAuthToken { value, .. } => {
                if value.as_persisted_str().trim().is_empty() || value.is_on_demand() {
                    return Err(ConfigError::msg(format!(
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
                return Err(ConfigError::msg(format!(
                    "account {id:?} has an empty model"
                )));
            }
            if base_url
                .as_deref()
                .is_some_and(|url| !Self::is_valid_http_endpoint(url))
            {
                return Err(ConfigError::msg(format!(
                    "account {id:?} requires an HTTP(S) endpoint"
                )));
            }
        }
        if !Agent::ALL.iter().any(|a| self.compatible_agent(*a)) {
            return Err(ConfigError::msg(format!(
                "account {id:?} has no compatible agent"
            )));
        }
        Ok(())
    }

    /// Runtime credential transport mode.
    pub const fn auth_mode(&self) -> AuthForwardMode {
        match self.credential {
            AccountCredential::Profile { .. } => AuthForwardMode::Sync,
            AccountCredential::ApiKey { .. } => AuthForwardMode::ApiKey,
            AccountCredential::OAuthToken { .. } => AuthForwardMode::OAuthToken,
        }
    }
    /// Exact source directory for profile credentials.
    pub fn source_directory(&self) -> Option<&Path> {
        match &self.credential {
            AccountCredential::Profile { directory, .. } => Some(directory),
            _ => None,
        }
    }
    /// Build unresolved credential environment for the selected agent.
    ///
    /// # Errors
    /// Rejects incompatible agent/provider combinations.
    pub fn credential_env(&self, agent: Agent) -> ConfigResult<BTreeMap<String, EnvValue>> {
        self.credential_env_for_endpoint(agent, None)
    }

    /// Build unresolved credential environment for one resolved instance.
    ///
    /// `base_url` is the effective instance endpoint. When it is `None`, the
    /// account endpoint or provider default is used. Endpoint-bearing
    /// variables are only emitted for clients whose credential contract uses
    /// environment routing; private provider-config clients apply the same
    /// value in their instance-specific config file.
    ///
    /// # Errors
    /// Rejects incompatible agent/provider combinations and invalid or
    /// unsupported endpoint overrides.
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
            return Err(ConfigError::msg(format!(
                "account {:?} cannot authenticate {agent}",
                self.name
            )));
        }
        let account_endpoint = match &self.credential {
            AccountCredential::ApiKey { base_url, .. } => base_url.as_deref(),
            AccountCredential::Profile { .. } | AccountCredential::OAuthToken { .. } => None,
        };
        let endpoint = endpoint_override.or(account_endpoint);
        if endpoint.is_some_and(|url| !Self::is_valid_http_endpoint(url)) {
            return Err(ConfigError::msg(format!(
                "account {:?} requires an HTTP(S) endpoint",
                self.name
            )));
        }
        let mut env = BTreeMap::new();
        match &self.credential {
            AccountCredential::Profile { .. } => {}
            AccountCredential::OAuthToken { value, .. } => {
                env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), value.clone());
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
                    return Err(ConfigError::msg(format!(
                        "account {:?} requires an explicit model for {agent}",
                        self.name
                    )));
                }
                let key = self.api_key_variable(agent);
                env.insert(key.into(), value.clone());
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
                    return Err(ConfigError::msg(format!(
                        "account {:?} has an endpoint override for {agent}, but that provider configuration is unsupported",
                        self.name
                    )));
                }
                // OpenCode writes its endpoint to private provider
                // configuration. Omp/Hermes endpoint overrides were rejected
                // above until their provider-config writers exist; neither
                // client receives an endpoint through ambient env.
                if !matches!(agent, Agent::Opencode | Agent::Omp | Agent::Hermes)
                    && let Some(url) = endpoint.or(default_url)
                {
                    let name = match agent {
                        Agent::Claude => "ANTHROPIC_BASE_URL",
                        Agent::Codex => "OPENAI_BASE_URL",
                        Agent::Kimi => "KIMI_BASE_URL",
                        _ => {
                            return Err(ConfigError::msg(
                                "endpoint overrides are unsupported for this agent",
                            ));
                        }
                    };
                    env.insert(name.into(), EnvValue::from(url));
                }
            }
        }
        Ok(env)
    }
}

/// Stable, secret-free identity for the credential source used by an account.
///
/// The fields intentionally mirror `same_credential_source`: API-key model
/// overrides do not identify a credential source, while the full persisted
/// `EnvValue` does. The digest lets removal tombstones survive without keeping
/// literal credentials in a second config field. Profile directories and XDG
/// roots are canonicalized for identity before hashing.
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
            hash_component(digest, &value.path);
            hash_optional_component(digest, value.account.as_deref());
            hash_component(digest, if value.on_demand { "true" } else { "false" });
        }
    }
}
/// Validate a stable, filesystem-safe account identifier.
///
/// # Errors
/// IDs must start with an ASCII lowercase letter or digit, followed by lowercase letters, digits, hyphens or underscores.
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
        return Err(ConfigError::msg(
            "account ID must be a lowercase slug of 1–64 characters",
        ));
    }
    Ok(())
}
/// Resolve only an authorized account. Global selections never expand workspace access.
///
/// # Errors
/// Fails for unknown workspaces/accounts, unauthorized bindings or ambiguous selection.
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
        // Keep an inherited global binding in the chain. It is an explicit
        // account selection, so an unauthorized ID must fail below instead
        // of disappearing and selecting another workspace account.
        .or_else(|| cfg.account_bindings.get(&agent));
    if let Some(id) = binding {
        if ws.is_some_and(|w| !w.accounts.contains(id)) {
            return Err(ConfigError::msg(format!(
                "account {id:?} is not assigned to this workspace"
            )));
        }
        let account = cfg
            .accounts
            .get(id)
            .ok_or_else(|| ConfigError::msg(format!("unknown account {id:?}")))?;
        if !account.supports_agent(agent) {
            return Err(ConfigError::msg(format!(
                "account {id:?} does not support {agent}"
            )));
        }
        return Ok(Some(account));
    }
    let Some(ws) = ws else { return Ok(None) };
    for id in &ws.accounts {
        if !cfg.accounts.contains_key(id) {
            return Err(ConfigError::msg(format!("unknown account {id:?}")));
        }
    }
    let mut candidates = ws
        .accounts
        .iter()
        .filter_map(|id| cfg.accounts.get(id))
        .filter(|a| a.supports_agent(agent));
    let selected = candidates.next();
    if candidates.next().is_some() {
        return Err(ConfigError::msg(format!(
            "multiple accounts support {agent}; select an account binding"
        )));
    }
    Ok(selected)
}
/// Current bootstrap sentinel schema version.
pub const BOOTSTRAP_VERSION: u32 = 1;

/// First-run bootstrap sentinel (`[bootstrap]` in `config.toml`).
///
/// Distinguishes a genuine fresh install (no config file), an
/// installer-created empty config (`fresh_install = true`, scan once),
/// and an older installation (no sentinel: already initialized, never
/// rescan — the migration stamps `fresh_install = false` explicitly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapState {
    /// Sentinel schema version ([`BOOTSTRAP_VERSION`]).
    pub version: u32,
    /// True only when an installer pre-created an empty config that
    /// still needs its first discovery scan.
    #[serde(default)]
    pub fresh_install: bool,
}

impl BootstrapState {
    /// Sentinel for a completed (or migrated) initialization.
    pub const fn initialized() -> Self {
        Self {
            version: BOOTSTRAP_VERSION,
            fresh_install: false,
        }
    }
}

/// Named agent/account/model template: one launchable agent instance.
///
/// Several configurations may share one account (and its quota); each
/// configuration pins the agent plus optional model/endpoint overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfiguration {
    /// Client that runs this instance.
    pub agent: Agent,
    /// Registered account supplying credentials.
    pub account: String,
    /// Model override; empty falls back to the account/client default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Endpoint override; empty falls back to the account/client default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Explicit instance label; default derives `{Agent} · {account name}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_label: Option<String>,
    /// Imported shell-wrapper provenance. Arbitrary wrappers are not a safe
    /// capsule launch transport, so configurations carrying this field fail
    /// validation and launch resolution before any instance is admitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoked_via_wrapper: Option<WrapperSpec>,
}

impl AgentConfiguration {
    /// Validate this configuration against the account registry.
    ///
    /// # Errors
    /// Rejects bad IDs, unknown/disabled/incompatible accounts, and bad overrides.
    pub fn validate(
        &self,
        id: &str,
        accounts: &BTreeMap<String, AccountConfig>,
    ) -> ConfigResult<()> {
        validate_account_id(id).map_err(|_| {
            ConfigError::msg("configuration ID must be a lowercase slug of 1–64 characters")
        })?;
        let account = accounts
            .get(&self.account)
            .ok_or_else(|| ConfigError::msg(format!("unknown account {:?}", self.account)))?;
        if !account.supports_agent(self.agent) {
            return Err(ConfigError::msg(format!(
                "account {:?} is not authorized for {}",
                self.account, self.agent
            )));
        }
        if self
            .model
            .as_deref()
            .is_some_and(|model| model.trim().is_empty())
        {
            return Err(ConfigError::msg(format!(
                "configuration {id:?} has an empty model"
            )));
        }
        if self
            .base_url
            .as_deref()
            .is_some_and(|url| !AccountConfig::is_valid_http_endpoint(url))
        {
            return Err(ConfigError::msg(format!(
                "configuration {id:?} requires an HTTP(S) endpoint"
            )));
        }
        if self.base_url.is_some() && matches!(self.agent, Agent::Omp | Agent::Hermes) {
            return Err(ConfigError::msg(format!(
                "configuration {id:?} has an endpoint override for {}, but that provider configuration is unsupported",
                self.agent
            )));
        }
        if self
            .display_label
            .as_deref()
            .is_some_and(|label| label.trim().is_empty())
        {
            return Err(ConfigError::msg(format!(
                "configuration {id:?} has an empty display label"
            )));
        }
        self.validate_launch_transport(id)?;
        Ok(())
    }

    /// Reject launch settings the capsule cannot execute safely.
    fn validate_launch_transport(&self, id: &str) -> ConfigResult<()> {
        if self.invoked_via_wrapper.is_some() {
            return Err(ConfigError::msg(format!(
                "configuration {id:?} declares an unsupported shell wrapper; jackin capsule launches cannot execute arbitrary host wrappers safely"
            )));
        }
        Ok(())
    }
}

/// One resolved launch instance: a configuration bound to an authorized account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInstance {
    /// Configuration ID (explicit, or synthesized `{account}@{agent}`).
    pub config_id: String,
    /// Client that runs this instance.
    pub agent: Agent,
    /// Registered account supplying credentials.
    pub account_id: String,
    /// Effective model (configuration override, else account default).
    pub model: Option<String>,
    /// Effective endpoint (configuration override, else account default).
    pub base_url: Option<String>,
    /// Explicit profile XDG roots, when the selected account carries them.
    /// These are threaded into selected auth provisioning; they are never
    /// inferred from ambient process environment.
    pub xdg_roots: Option<XdgRoots>,
    /// Instance label (`{Agent} · {account name}` unless overridden).
    pub label: String,
    /// True when synthesized from a binding/sole-eligible fallback.
    pub synthesized: bool,
}

impl ResolvedInstance {
    /// Bind an explicit configuration to its resolved account values.
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

    /// Synthesize the default instance for one account/agent pair.
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

/// Resolve the ordered launch instances for a workspace/role selection.
///
/// Precedence: one-launch selection → role `default_launch` → workspace
/// `default_launch` → global `default_launch` → per-agent
/// `account_bindings` (role → workspace → global, when `agent` is
/// known) → sole eligible instance. A full launch list beats a bare
/// per-agent account preference at any scope; the committed launch
/// agent scopes the binding/sole-eligible fallbacks so fast start
/// honors a valid default instead of prompting whenever several
/// accounts exist. An explicit scope replaces inherited scopes (no
/// union). Explicit selections validate atomically against
/// authorization and compatibility: invalid entries fail the whole
/// launch, never fall back silently. Inherited global candidates
/// filter by workspace authorization. An explicit empty list resolves
/// to no instances (shell-only launches accept that; agent launches
/// reject it).
///
/// # Errors
/// Fails for unknown workspaces/configurations/accounts, unauthorized
/// or incompatible selections, zero eligible accounts, and ambiguous
/// (picker-needed) fallbacks.
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
                return Err(ConfigError::msg(format!(
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
            let config = cfg
                .agent_configurations
                .get(id)
                .ok_or_else(|| ConfigError::msg(format!("unknown agent configuration {id:?}")))?;
            // Role/workspace scopes validate atomically like bindings;
            // inherited global candidates filter by authorization.
            if scope_is_global && !authorized(&config.account) {
                continue;
            }
            instances.push(bind_explicit(cfg, ws, id)?);
        }
        return checked_launch_instances(instances);
    }

    // No launch list anywhere: per-agent account bindings are the
    // global defaults layer (role → workspace → global), validated
    // atomically — a valid default wins over prompting whenever the
    // committed launch agent is known.
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
    // No defaults anywhere: sole eligible instance wins, ambiguity needs a picker.
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
                    .ok_or_else(|| ConfigError::msg(format!("unknown account {id:?}")))?;
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
        return Err(ConfigError::msg("no eligible account for this launch"));
    }
    if eligible.len() > 1 {
        return Err(ConfigError::msg(
            "multiple accounts are eligible; select launch configurations",
        ));
    }
    Ok(eligible)
}

/// Reject same-agent instance sets the container cannot isolate.
/// Several admitted instances for one agent need a dedicated config-folder
/// env var (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_HOME`, …) so
/// each pane gets its own credentials and history. Agents without one
/// cannot isolate instances; `XDG_*`-root agents would also redirect
/// unrelated XDG consumers of the pane process. Both fail closed with
/// the exact reason.
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
                return Err(ConfigError::msg(format!(
                    "agent {agent} admits only one account per container: \
                     isolating several accounts needs {name}, which would also \
                     redirect unrelated XDG consumers of the pane process"
                )));
            }
            None => {
                return Err(ConfigError::msg(format!(
                    "agent {agent} admits only one account per container: \
                     it has no config-folder env var to isolate instances"
                )));
            }
        }
    }
    Ok(instances)
}

/// Bind one explicit configuration ID, validating authorization and compatibility.
fn bind_explicit(
    cfg: &AppConfig,
    ws: Option<&WorkspaceConfig>,
    id: &str,
) -> ConfigResult<ResolvedInstance> {
    let config = cfg
        .agent_configurations
        .get(id)
        .ok_or_else(|| ConfigError::msg(format!("unknown agent configuration {id:?}")))?;
    config.validate_launch_transport(id)?;
    if let Some(w) = ws
        && !w.accounts.contains(&config.account)
    {
        return Err(ConfigError::msg(format!(
            "account {:?} is not assigned to this workspace",
            config.account
        )));
    }
    let account = cfg
        .accounts
        .get(&config.account)
        .ok_or_else(|| ConfigError::msg(format!("unknown account {:?}", config.account)))?;
    if !account.supports_agent(config.agent) {
        return Err(ConfigError::msg(format!(
            "account {:?} is not authorized for {}",
            config.account, config.agent
        )));
    }
    Ok(ResolvedInstance::bind(id, config, account))
}
impl AppConfig {
    /// Validate registry credentials and all account references.
    ///
    /// # Errors
    /// Rejects invalid credentials, unknown IDs, disabled or incompatible bindings, and workspace authorization violations.
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
                    return Err(ConfigError::msg(format!(
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

    /// Validate registry credentials plus binding and allowlist integrity.
    ///
    /// This is the editor-open migration gate scope: it covers everything
    /// the pre-multi-account gate enforced, while launch-instance
    /// references (`agent_configurations`, `default_launch`) are left to
    /// save/load validation, so the editor stays usable as a repair tool
    /// for a config whose instance account is not registered yet.
    pub(crate) fn validate_registry_and_bindings(&self) -> ConfigResult<()> {
        for (id, account) in &self.accounts {
            account.validate(id)?;
        }
        self.check_account_bindings(&self.account_bindings, None)?;
        for ws in self.workspaces.values() {
            let mut seen = BTreeSet::new();
            for id in &ws.accounts {
                if !self.accounts.contains_key(id) || !seen.insert(id) {
                    return Err(ConfigError::msg(format!(
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

    /// Reject bindings that name unknown accounts or escape the workspace allowlist.
    fn check_account_bindings(
        &self,
        bindings: &BTreeMap<Agent, String>,
        allowed: Option<&Vec<String>>,
    ) -> ConfigResult<()> {
        for (agent, id) in bindings {
            let account = self
                .accounts
                .get(id)
                .ok_or_else(|| ConfigError::msg(format!("unknown account {id:?}")))?;
            if !account.supports_agent(*agent) || allowed.is_some_and(|ids| !ids.contains(id)) {
                return Err(ConfigError::msg(format!(
                    "account {id:?} is not authorized for {agent}"
                )));
            }
        }
        Ok(())
    }

    /// Validate one `default_launch` list: known IDs, no duplicates, and
    /// (for workspace/role scopes) accounts inside the workspace allowlist.
    fn validate_launch_list(
        &self,
        ids: Option<&[String]>,
        allowed: Option<&Vec<String>>,
    ) -> ConfigResult<()> {
        let Some(ids) = ids else { return Ok(()) };
        let mut seen = BTreeSet::new();
        for id in ids {
            if !seen.insert(id) {
                return Err(ConfigError::msg(format!(
                    "duplicate launch configuration {id:?}"
                )));
            }
            let config = self
                .agent_configurations
                .get(id)
                .ok_or_else(|| ConfigError::msg(format!("unknown agent configuration {id:?}")))?;
            if allowed.is_some_and(|ids| !ids.contains(&config.account)) {
                return Err(ConfigError::msg(format!(
                    "account {:?} is not assigned to this workspace",
                    config.account
                )));
            }
        }
        Ok(())
    }

    /// Prune bindings to the given account ID across global, workspace, and role scopes.
    pub fn prune_account_bindings(&mut self, id: &str) {
        self.account_bindings.retain(|_, selected| selected != id);
        for ws in self.workspaces.values_mut() {
            ws.account_bindings.retain(|_, selected| selected != id);
            for role in ws.roles.values_mut() {
                role.account_bindings.retain(|_, selected| selected != id);
            }
        }
    }

    /// Drop agent configurations using the given account and scrub
    /// `default_launch` lists of the removed configuration IDs.
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
