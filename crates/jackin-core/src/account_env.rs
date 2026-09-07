// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential and routing environment owned exclusively by account selection.

/// Account routing names in addition to the provider credential catalog.
pub const ACCOUNT_ROUTING_ENV_NAMES: &[&str] = &[
    "MOONSHOT_API_KEY",
    "MINIMAX_API_TOKEN",
    "MINIMAX_CODING_API_KEY",
    "Z_AI_API_KEY",
    "ZHIPU_API_KEY",
    "KIMI_BASE_URL",
    "KIMI_AUTH_TOKEN",
    "kimi_auth_token",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "OPENAI_BASE_URL",
    "OPENAI_API_BASE",
    "AMP_URL",
    "XAI_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
];
/// Every credential/routing environment name stripped before spawning a pane.
pub fn account_env_names() -> impl Iterator<Item = &'static str> {
    crate::USAGE_CREDENTIAL_ENV_REGISTRY
        .iter()
        .map(|entry| entry.name)
        .chain(ACCOUNT_ROUTING_ENV_NAMES.iter().copied())
}
/// Whether a variable is exclusively controlled by selected accounts.
pub fn is_account_env(name: &str) -> bool {
    account_env_names().any(|owned| owned == name)
}
