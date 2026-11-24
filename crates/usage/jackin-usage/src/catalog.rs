// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::accounts::UsageProvider;

#[derive(Debug, Clone, Copy)]
pub struct UsageProduct {
    pub id: &'static str,
    pub name: &'static str,
    pub connector: Option<UsageProvider>,
    pub unavailable_reason: Option<&'static str>,
}

impl UsageProvider {
    pub fn metric_group(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::ChatGpt => "ChatGPT",
            Self::Muse => "Muse",
            Self::Grok => "Grok",
            Self::Kimi => "Kimi",
            Self::Zai => "Z.ai",
            Self::MiniMax => "MiniMax",
            Self::Mimo => "MiMo",
            Self::Antigravity => "Antigravity",
            Self::OpenCode => "OpenCode",
            Self::OpenRouter => "OpenRouter",
        }
    }
}

pub fn providers() -> impl Iterator<Item = UsageProduct> {
    UsageProvider::ALL.into_iter().map(|provider| UsageProduct {
        id: provider.as_str(),
        name: match provider {
            UsageProvider::ChatGpt => "ChatGPT / Codex",
            UsageProvider::Antigravity => "Antigravity (Google AI)",
            UsageProvider::Zai => "Z.AI",
            _ => provider.metric_group(),
        },
        connector: Some(provider),
        unavailable_reason: None,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct InfoProduct {
    pub id: &'static str,
    pub name: &'static str,
    pub reason: &'static str,
    pub hint: &'static str,
}

pub fn info_only() -> impl Iterator<Item = InfoProduct> {
    [InfoProduct {
        id: "gemini-app",
        name: "Gemini app (web)",
        reason: "chat limits are not tracked; there is no usage source",
        hint: "Google AI plan quotas appear under Antigravity (Google AI).",
    }]
    .into_iter()
}
