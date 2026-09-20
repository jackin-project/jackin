// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential and routing environment owned exclusively by account selection.

/// Account routing names and client state roots outside the provider credential catalog.
pub const ACCOUNT_ROUTING_ENV_NAMES: &[&str] = &[
    // Credential aliases recognized by account discovery or provider clients.
    "ANTHROPIC_AUTH_TOKEN",
    "MOONSHOT_API_KEY",
    "MINIMAX_API_TOKEN",
    "MINIMAX_CODING_API_KEY",
    "Z_AI_API_KEY",
    "ZHIPU_API_KEY",
    "KIMI_AUTH_TOKEN",
    "kimi_auth_token",
    // Endpoint aliases recognized by provider account import.
    "OPENAI_API_URL",
    "AMP_BASE_URL",
    "AMP_API_URL",
    "XAI_API_BASE",
    "XAI_API_URL",
    "OPENCODE_BASE_URL",
    "OPENCODE_API_BASE",
    "OPENCODE_API_URL",
    "KIMI_CODE_BASE_URL",
    "MOONSHOT_BASE_URL",
    "MOONSHOT_API_BASE",
    "MOONSHOT_API_URL",
    "ZAI_BASE_URL",
    "Z_AI_BASE_URL",
    "ZHIPU_BASE_URL",
    "ZAI_API_BASE",
    "ZAI_API_URL",
    "MINIMAX_BASE_URL",
    "MINIMAX_API_BASE",
    "MINIMAX_API_URL",
    "GEMINI_BASE_URL",
    "GOOGLE_BASE_URL",
    "GEMINI_API_BASE",
    "GEMINI_API_URL",
    "CURSOR_BASE_URL",
    "CURSOR_API_BASE",
    "CURSOR_API_URL",
    "META_BASE_URL",
    "META_API_BASE",
    "META_API_URL",
    "OPENROUTER_BASE_URL",
    "OPENROUTER_API_URL",
    // Account-specific client configuration and state roots.
    "HOME",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "KIMI_HOME",
    "AMP_HOME",
    "GEMINI_CLI_HOME",
    "CURSOR_CONFIG_DIR",
    "HERMES_HOME",
    "PI_CODING_AGENT_DIR",
    "OMP_PROFILE",
    "OPENCODE_CONFIG",
    "OPENCODE_CONFIG_DIR",
    "OPENCODE_CONFIG_CONTENT",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
    // Claude's native model selectors.
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    // Provider endpoint variables emitted by selected account configuration.
    "KIMI_BASE_URL",
    "ANTHROPIC_BASE_URL",
    "OPENAI_BASE_URL",
    "OPENAI_API_BASE",
    "AMP_URL",
    "XAI_BASE_URL",
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

#[cfg(test)]
mod tests;
