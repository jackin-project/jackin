// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Named credentials and workspace account authorization.

use crate::{AppConfig, ConfigError, ConfigResult};
use jackin_core::{Agent, AuthForwardMode, EnvValue, WorkspaceName};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(crate) mod discovery;

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
}
impl AiProvider {
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
        }
    }
    /// Native service for an agent's profile.
    pub const fn for_agent(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Anthropic,
            Agent::Codex => Self::OpenAi,
            Agent::Amp => Self::Amp,
            Agent::Kimi => Self::Moonshot,
            Agent::Opencode => Self::Opencode,
            Agent::Grok => Self::Xai,
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
            _ => Err(ConfigError::msg(format!("unknown AI provider {value:?}"))),
        }
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
impl std::fmt::Debug for AccountCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Profile { agent, directory } => f
                .debug_struct("Profile")
                .field("agent", agent)
                .field("directory", directory)
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
            AccountCredential::Profile { agent: owner, .. } => {
                *owner == agent && self.provider == AiProvider::for_agent(agent)
            }
            AccountCredential::OAuthToken { agent: owner, .. } => {
                *owner == agent && agent == Agent::Claude && self.provider == AiProvider::Anthropic
            }
            AccountCredential::ApiKey { .. } => {
                self.provider == AiProvider::for_agent(agent)
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
                        Agent::Opencode => !matches!(self.provider, AiProvider::Amp),
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
            Agent::Opencode => match self.provider {
                AiProvider::Anthropic => "ANTHROPIC_API_KEY",
                AiProvider::OpenAi => "OPENAI_API_KEY",
                AiProvider::Xai => "XAI_API_KEY",
                AiProvider::Moonshot => "MOONSHOT_API_KEY",
                AiProvider::Zai => "ZHIPU_API_KEY",
                AiProvider::Minimax => "MINIMAX_API_KEY",
                _ => "OPENCODE_API_KEY",
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
            _ => None,
        }
    }

    fn validate(&self, id: &str) -> ConfigResult<()> {
        validate_account_id(id)?;
        if self.name.trim().is_empty() {
            return Err(ConfigError::msg(format!(
                "account {id:?} has an empty name"
            )));
        }
        match &self.credential {
            AccountCredential::Profile { agent, directory } => {
                if !directory.is_absolute() || !self.compatible_agent(*agent) {
                    return Err(ConfigError::msg(format!("invalid profile account {id:?}")));
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
            if base_url.as_deref().is_some_and(|url| {
                !(url.starts_with("https://") || url.starts_with("http://"))
                    || url.contains(char::is_whitespace)
            }) {
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
        if !self.supports_agent(agent) {
            return Err(ConfigError::msg(format!(
                "account {:?} cannot authenticate {agent}",
                self.name
            )));
        }
        let mut env = BTreeMap::new();
        match &self.credential {
            AccountCredential::Profile { .. } => {}
            AccountCredential::OAuthToken { value, .. } => {
                env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), value.clone());
            }
            AccountCredential::ApiKey {
                value,
                base_url,
                model,
            } => {
                if matches!(agent, Agent::Claude | Agent::Codex)
                    && self.provider != AiProvider::for_agent(agent)
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
                // OpenCode endpoints are written to its private provider configuration.
                if agent != Agent::Opencode
                    && let Some(url) = base_url.as_deref().or(default_url)
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
        .or_else(|| {
            cfg.account_bindings
                .get(&agent)
                .filter(|id| ws.is_none_or(|w| w.accounts.contains(id)))
        });
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
impl AppConfig {
    /// Validate registry credentials and all account references.
    ///
    /// # Errors
    /// Rejects invalid credentials, unknown IDs, incompatible bindings and workspace authorization violations.
    pub fn validate_accounts(&self) -> ConfigResult<()> {
        for (id, account) in &self.accounts {
            account.validate(id)?;
        }
        let check = |bindings: &BTreeMap<Agent, String>,
                     allowed: Option<&Vec<String>>|
         -> ConfigResult<()> {
            for (agent, id) in bindings {
                let account = self
                    .accounts
                    .get(id)
                    .ok_or_else(|| ConfigError::msg(format!("unknown account {id:?}")))?;
                if !account.compatible_agent(*agent) || allowed.is_some_and(|ids| !ids.contains(id))
                {
                    return Err(ConfigError::msg(format!(
                        "account {id:?} is not authorized for {agent}"
                    )));
                }
            }
            Ok(())
        };
        check(&self.account_bindings, None)?;
        for ws in self.workspaces.values() {
            let mut seen = BTreeSet::new();
            for id in &ws.accounts {
                if !self.accounts.contains_key(id) || !seen.insert(id) {
                    return Err(ConfigError::msg(format!(
                        "unknown or duplicate workspace account {id:?}"
                    )));
                }
            }
            check(&ws.account_bindings, Some(&ws.accounts))?;
            for role in ws.roles.values() {
                check(&role.account_bindings, Some(&ws.accounts))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
