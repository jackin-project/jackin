// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthForwardMode {
    #[default]
    #[serde(rename = "sync")]
    Sync,

    #[serde(rename = "api_key")]
    ApiKey,

    #[serde(rename = "oauth_token")]
    OAuthToken,

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
