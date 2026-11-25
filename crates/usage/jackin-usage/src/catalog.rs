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

pub fn providers() -> impl Iterator<Item = UsageProduct> {
    UsageProvider::ALL.into_iter().map(|provider| {
        let name = match provider {
            UsageProvider::Claude => "Claude",
            UsageProvider::ChatGpt => "ChatGPT / Codex",
            UsageProvider::Muse => "Muse",
            UsageProvider::Grok => "Grok",
            UsageProvider::Kimi => "Kimi",
            UsageProvider::Zai => "Z.AI",
            UsageProvider::MiniMax => "MiniMax",
            UsageProvider::Mimo => "MiMo",
            UsageProvider::Gemini => "Gemini Code Assist",
            UsageProvider::OpenCode => "OpenCode",
            UsageProvider::OpenRouter => "OpenRouter",
        };
        UsageProduct {
            id: provider.as_str(),
            name,
            connector: Some(provider),
            unavailable_reason: None,
        }
    }).chain(std::iter::once(UsageProduct {
        id: "google-ai-ultra",
        name: "Google AI Ultra (Gemini)",
        connector: None,
        unavailable_reason: Some("Jackin cannot read this subscription’s quota. In the Gemini app, open Settings → Usage Limits."),
    }))
}
