// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::Secret;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::ConnectionError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageProvider {
    Claude,
    #[serde(rename = "codex")]
    ChatGpt,
    Muse,
    Grok,
    Kimi,
    Zai,
    MiniMax,
    Mimo,
    Antigravity,
    OpenCode,
    OpenRouter,
}

impl UsageProvider {
    pub const ALL: [Self; 11] = [
        Self::Claude,
        Self::ChatGpt,
        Self::Muse,
        Self::Grok,
        Self::Kimi,
        Self::Zai,
        Self::MiniMax,
        Self::Mimo,
        Self::Antigravity,
        Self::OpenCode,
        Self::OpenRouter,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::ChatGpt => "codex",
            Self::Muse => "muse",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::Zai => "zai",
            Self::MiniMax => "minimax",
            Self::Mimo => "mimo",
            Self::Antigravity => "antigravity",
            Self::OpenCode => "opencode",
            Self::OpenRouter => "openrouter",
        }
    }
}

impl fmt::Display for UsageProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for UsageProvider {
    type Err = ConnectionError;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == name)
            .ok_or(ConnectionError::Provider)
    }
}

#[derive(Clone)]
pub enum CandidateIdentity {
    OAuth(Option<String>),
    ApiKey(KeyFingerprint),
}

#[derive(Clone)]
pub struct KeyFingerprint(String);

impl fmt::Debug for KeyFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyFingerprint(<redacted>)")
    }
}

impl fmt::Debug for CandidateIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OAuth(_) => "OAuth(<redacted>)",
            Self::ApiKey(_) => "ApiKey(<redacted>)",
        })
    }
}

impl CandidateIdentity {
    pub fn api_key(key: &Secret) -> Self {
        Self::ApiKey(KeyFingerprint(
            URL_SAFE_NO_PAD.encode(Sha256::digest(key.expose().as_bytes())),
        ))
    }

    pub(super) fn parts(
        &self,
        provider: UsageProvider,
    ) -> Result<(&'static str, Option<&str>), ConnectionError> {
        use UsageProvider::{
            Antigravity, ChatGpt, Claude, Grok, Kimi, Mimo, MiniMax, Muse, OpenCode, OpenRouter,
            Zai,
        };
        match (self, provider) {
            (
                Self::OAuth(id),
                Claude | ChatGpt | Muse | Grok | OpenCode | Kimi | MiniMax | Antigravity,
            ) => Ok(("oauth", id.as_deref().filter(|id| !id.is_empty()))),
            (Self::ApiKey(key), Zai | OpenRouter | Mimo | MiniMax | Kimi) => {
                Ok(("key", Some(&key.0)))
            }
            _ => Err(ConnectionError::CredentialKind),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountPreferences {
    pub pinned: BTreeSet<String>,
    pub hidden: BTreeSet<String>,
    pub metric_order: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectedAccount {
    pub id: Uuid,
    pub provider: UsageProvider,
    pub label: String,
    pub position: i64,
    pub preferences: AccountPreferences,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CredentialVersion(pub(super) Uuid);
