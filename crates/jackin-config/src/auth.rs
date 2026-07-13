// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Auth configuration types for the three-layer resolver.
//!
//! These types carry the auth-forwarding mode for a specific agent at a
//! specific configuration layer (global, workspace, workspace×role). They
//! do not hold credential values — only the *policy* for how credentials
//! are forwarded.

use jackin_core::{AuthForwardMode, EnvValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Per-agent auth configuration wrapper.
///
/// Carries `auth_forward` mode and optional sync source folder at any layer
/// of the resolver.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentAuthConfig {
    /// How host credentials are forwarded for this agent at this layer.
    #[serde(default)]
    pub auth_forward: AuthForwardMode,
    /// Optional override for the host folder `sync` reads credentials from.
    ///
    /// `None` = inherit from the next lower layer → global → per-agent hardcoded
    /// default.  The folder path is stored as the operator chose it; when set,
    /// provisioning treats it as the agent's credential/config directory itself
    /// (for example a `CODEX_HOME` or `CLAUDE_CONFIG_DIR` value), not as a
    /// replacement home directory.
    ///
    /// **Precedence** (most-specific wins): workspace-role → workspace → global →
    /// per-agent hardcoded.  An absent value at a layer means "inherit from below."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_source_dir: Option<PathBuf>,
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
    /// How host `gh` auth state is forwarded into the container.
    #[serde(default)]
    pub auth_forward: GithubAuthMode,
    /// Operator env for `token` mode (`GH_TOKEN`, optional `GH_HOST`, etc.).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
}
