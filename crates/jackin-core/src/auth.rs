// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `AuthForwardMode`: controls how host agent credentials are forwarded into
//! role containers.
//!
//! Wire format uses explicit `rename` so the TOML/JSON names the operator
//! types match what serde reads.

use serde::{Deserialize, Serialize};

/// Controls how the host's agent credentials are forwarded into role
/// containers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthForwardMode {
    /// Overwrite container auth from host on each launch when host auth
    /// exists; preserve container auth when host auth is absent.
    #[default]
    #[serde(rename = "sync")]
    Sync,
    /// Use a short-lived API key sourced from the operator-resolved env
    /// (e.g. `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `AMP_API_KEY`). The
    /// role state directory is provisioned empty; the agent inside the
    /// container reads the key from its process environment.
    #[serde(rename = "api_key")]
    ApiKey,
    /// Use a long-lived OAuth token sourced from the operator-resolved env
    /// (e.g. `CLAUDE_CODE_OAUTH_TOKEN`). The role state directory is
    /// provisioned empty; the agent inside the container reads the token
    /// from its process environment.
    #[serde(rename = "oauth_token")]
    OAuthToken,
    /// Revoke any forwarded auth and never copy — container starts with `{}`.
    #[serde(rename = "ignore")]
    Ignore,
}

impl std::fmt::Display for AuthForwardMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sync => write!(f, "sync"),
            Self::ApiKey => write!(f, "api_key"),
            Self::OAuthToken => write!(f, "oauth_token"),
            Self::Ignore => write!(f, "ignore"),
        }
    }
}

impl std::str::FromStr for AuthForwardMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sync" => Ok(Self::Sync),
            "api_key" => Ok(Self::ApiKey),
            "oauth_token" => Ok(Self::OAuthToken),
            "ignore" => Ok(Self::Ignore),
            other => Err(format!(
                "invalid auth_forward mode {other:?}; expected one of: sync, api_key, oauth_token, ignore"
            )),
        }
    }
}
