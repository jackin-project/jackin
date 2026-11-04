// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::agent::Agent;

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
    Muse,
    Opencode,
    Openrouter,
    Zai,
}

impl Provider {
    pub const ALL: [Provider; 14] = [
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
            Provider::Muse => "muse",
            Provider::Opencode => "opencode",
            Provider::Openrouter => "openrouter",
            Provider::Zai => "zai",
        }
    }

    pub fn agent(self) -> Option<Agent> {
        match self {
            Provider::Codex => Some(Agent::Codex),
            Provider::Amp
            | Provider::Antigravity
            | Provider::Claude
            | Provider::Cursor
            | Provider::Gemini
            | Provider::Grok
            | Provider::Hermes
            | Provider::Kimi
            | Provider::Minimax
            | Provider::Muse
            | Provider::Opencode
            | Provider::Openrouter
            | Provider::Zai => None,
        }
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
            | Provider::Openrouter
            | Provider::Zai => None,
        }
    }
}
