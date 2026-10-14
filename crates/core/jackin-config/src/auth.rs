// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::EnvValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GithubAuthMode {
    #[default]
    #[serde(rename = "sync")]
    Sync,

    #[serde(rename = "token")]
    Token,

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

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubAuthConfig {
    #[serde(default)]
    pub auth_forward: GithubAuthMode,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
}
