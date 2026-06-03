//! Auth configuration types for the three-layer resolver.
//!
//! These types carry the auth-forwarding mode for a specific agent at a
//! specific configuration layer (global, workspace, workspace×role). They
//! do not hold credential values — only the *policy* for how credentials
//! are forwarded.

use jackin_core::{AuthForwardMode, EnvValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Per-agent auth configuration wrapper.
///
/// Carries `auth_forward` mode at any layer of the resolver.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentAuthConfig {
    #[serde(default)]
    pub auth_forward: AuthForwardMode,
}

/// Controls how the host's `gh` auth state reaches role containers.
///
/// Distinct from [`AuthForwardMode`] because GitHub has no `api_key` /
/// `oauth_token` distinction — all PATs are uniform.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GithubAuthMode {
    /// Materialize `~/.config/gh/hosts.yml` from the host's `gh` login.
    #[default]
    #[serde(rename = "sync")]
    Sync,
    /// Inject `GH_TOKEN` (and `GITHUB_TOKEN`) from the resolved env.
    #[serde(rename = "token")]
    Token,
    /// Wipe any forwarded `gh` state and never forward host auth.
    #[serde(rename = "ignore")]
    Ignore,
}

impl std::fmt::Display for GithubAuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sync => write!(f, "sync"),
            Self::Token => write!(f, "token"),
            Self::Ignore => write!(f, "ignore"),
        }
    }
}

impl std::str::FromStr for GithubAuthMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sync" => Ok(Self::Sync),
            "token" => Ok(Self::Token),
            "ignore" => Ok(Self::Ignore),
            other => Err(format!(
                "invalid github auth_forward mode {other:?}; expected one of: sync, token, ignore"
            )),
        }
    }
}

/// Operator-only `[github]` configuration block.
///
/// `env` holds the operator env map for `token` mode — must contain `GH_TOKEN`,
/// optionally `GH_HOST` (for GHE) and `GH_ENTERPRISE_TOKEN`. Values resolve
/// through the same `operator_env` dispatch as Claude/Codex auth env.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubAuthConfig {
    #[serde(default)]
    pub auth_forward: GithubAuthMode,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
}

/// Generates an `AgentAuthConfig` newtype that rejects `OAuthToken`.
///
/// Used for agents that do not support OAuth token forwarding:
/// `Codex`, `Amp`, `Kimi`, `OpenCode`.
macro_rules! agent_auth_config_no_oauth {
    ($name:ident, $agent:literal) => {
        #[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
        pub struct $name(pub AgentAuthConfig);

        impl $name {
            /// Construct, rejecting `OAuthToken`.
            pub fn new(cfg: AgentAuthConfig) -> Result<Self, &'static str> {
                if cfg.auth_forward == AuthForwardMode::OAuthToken {
                    return Err(concat!(
                        "auth_forward 'oauth_token' is not supported for ",
                        $agent
                    ));
                }
                Ok(Self(cfg))
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let cfg = AgentAuthConfig::deserialize(deserializer)?;
                if cfg.auth_forward == AuthForwardMode::OAuthToken {
                    return Err(serde::de::Error::custom(concat!(
                        "auth_forward 'oauth_token' is not supported for ",
                        $agent,
                        "; supported modes: sync, api_key, ignore"
                    )));
                }
                Ok(Self(cfg))
            }
        }

        impl std::ops::Deref for $name {
            type Target = AgentAuthConfig;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

agent_auth_config_no_oauth!(CodexAuthConfig, "codex");
agent_auth_config_no_oauth!(AmpAuthConfig, "amp");
agent_auth_config_no_oauth!(KimiAuthConfig, "kimi");
agent_auth_config_no_oauth!(OpencodeAuthConfig, "opencode");
