// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
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
