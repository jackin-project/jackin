// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `AuthKind` account classification helpers.

use super::super::AuthKind;

/// Each provider can own any number of independently named credentials.
pub const ACCOUNT_KINDS: &[AuthKind] = &[
    AuthKind::Claude,
    AuthKind::Codex,
    AuthKind::Amp,
    AuthKind::Kimi,
    AuthKind::Opencode,
    AuthKind::Grok,
    AuthKind::Antigravity,
    AuthKind::Gemini,
    AuthKind::Cursor,
    AuthKind::Muse,
    AuthKind::Omp,
    AuthKind::Hermes,
    AuthKind::Zai,
    AuthKind::Minimax,
];

pub fn account_kind(account: &jackin_config::AccountConfig) -> AuthKind {
    use jackin_config::AiProvider;
    // Owner-bearing credentials resolve by owner, not provider: two agents
    // share the Google provider, so provider-only mapping would rewrite an
    // Antigravity profile's owner on save.
    match &account.credential {
        jackin_config::AccountCredential::Profile { agent, .. }
        | jackin_config::AccountCredential::OAuthToken { agent, .. } => {
            if let Some(kind) = auth_kind_for_agent(*agent) {
                return kind;
            }
        }
        jackin_config::AccountCredential::ApiKey { .. } => {}
    }
    match account.provider {
        AiProvider::Anthropic => AuthKind::Claude,
        AiProvider::OpenAi => AuthKind::Codex,
        AiProvider::Amp => AuthKind::Amp,
        AiProvider::Moonshot => AuthKind::Kimi,
        AiProvider::Opencode => AuthKind::Opencode,
        AiProvider::Xai => AuthKind::Grok,
        // Ownerless Google keys show under Gemini; either Google agent can
        // consume them and the save roundtrips to the same provider.
        AiProvider::Google => AuthKind::Gemini,
        AiProvider::Cursor => AuthKind::Cursor,
        AiProvider::Meta => AuthKind::Muse,
        // Routed-provider accounts show under the first multi-provider kind;
        // the save roundtrips by provider, not kind.
        AiProvider::OpenRouter => AuthKind::Omp,
        AiProvider::Zai => AuthKind::Zai,
        AiProvider::Minimax => AuthKind::Minimax,
    }
}

/// Inverse of [`auth_kind_agent`](crate::tui::auth_config::auth_kind_agent).
pub(crate) fn auth_kind_for_agent(agent: jackin_core::Agent) -> Option<AuthKind> {
    use jackin_core::Agent;
    match agent {
        Agent::Claude => Some(AuthKind::Claude),
        Agent::Codex => Some(AuthKind::Codex),
        Agent::Amp => Some(AuthKind::Amp),
        Agent::Kimi => Some(AuthKind::Kimi),
        Agent::Opencode => Some(AuthKind::Opencode),
        Agent::Grok => Some(AuthKind::Grok),
        Agent::Antigravity => Some(AuthKind::Antigravity),
        Agent::Gemini => Some(AuthKind::Gemini),
        Agent::Cursor => Some(AuthKind::Cursor),
        Agent::Muse => Some(AuthKind::Muse),
        Agent::Omp => Some(AuthKind::Omp),
        Agent::Hermes => Some(AuthKind::Hermes),
    }
}
