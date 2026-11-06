// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Amp,
    Antigravity,
    Claude,
    Codex,
    Cursor,
    Gemini,
    Grok,
    Hermes,
    Kimi,
    Minimax,
    Mimo,
    Muse,
    Opencode,
    Openrouter,
    Zai,
}

impl Provider {
    pub const ALL: [Provider; 15] = [
        Provider::Amp,
        Provider::Antigravity,
        Provider::Claude,
        Provider::Codex,
        Provider::Cursor,
        Provider::Gemini,
        Provider::Grok,
        Provider::Hermes,
        Provider::Kimi,
        Provider::Minimax,
        Provider::Mimo,
        Provider::Muse,
        Provider::Opencode,
        Provider::Openrouter,
        Provider::Zai,
    ];
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Amp => "amp",
            Provider::Antigravity => "antigravity",
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Cursor => "cursor",
            Provider::Gemini => "gemini",
            Provider::Grok => "grok",
            Provider::Hermes => "hermes",
            Provider::Kimi => "kimi",
            Provider::Minimax => "minimax",
            Provider::Mimo => "mimo",
            Provider::Muse => "muse",
            Provider::Opencode => "opencode",
            Provider::Openrouter => "openrouter",
            Provider::Zai => "zai",
        }
    }

    pub fn tuples(self) -> Vec<&'static crate::agent::AgentTuple> {
        crate::agent::TUPLES
            .iter()
            .filter(|tuple| tuple.provider == self)
            .collect()
    }

    pub fn login_file(self) -> Option<&'static str> {
        match self {
            Provider::Amp => Some("secrets.json"),
            Provider::Claude => Some(".credentials.json"),
            Provider::Codex
            | Provider::Cursor
            | Provider::Grok
            | Provider::Muse
            | Provider::Opencode => Some("auth.json"),
            Provider::Gemini => Some("oauth_creds.json"),
            Provider::Kimi => Some("credentials/kimi-code.json"),
            Provider::Antigravity
            | Provider::Hermes
            | Provider::Minimax
            | Provider::Mimo
            | Provider::Openrouter
            | Provider::Zai => None,
        }
    }
}
