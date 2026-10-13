// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn account_env_registry_covers_provider_aliases_and_client_state_roots() {
    let names: std::collections::BTreeSet<_> = account_env_names().collect();
    for name in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "OPENAI_API_KEY",
        "AMP_API_KEY",
        "AMP_URL",
        "AMP_BASE_URL",
        "AMP_API_URL",
        "XAI_API_KEY",
        "GROK_DEPLOYMENT_KEY",
        "XAI_BASE_URL",
        "XAI_API_BASE",
        "XAI_API_URL",
        "OPENCODE_API_KEY",
        "OPENCODE_BASE_URL",
        "OPENCODE_API_BASE",
        "OPENCODE_API_URL",
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_CONFIG_CONTENT",
        "KIMI_API_KEY",
        "KIMI_CODE_API_KEY",
        "MOONSHOT_API_KEY",
        "KIMI_AUTH_TOKEN",
        "kimi_auth_token",
        "KIMI_BASE_URL",
        "KIMI_CODE_BASE_URL",
        "MOONSHOT_BASE_URL",
        "MOONSHOT_API_BASE",
        "MOONSHOT_API_URL",
        "ZAI_API_KEY",
        "Z_AI_API_KEY",
        "ZHIPU_API_KEY",
        "ZAI_BASE_URL",
        "Z_AI_BASE_URL",
        "ZHIPU_BASE_URL",
        "ZAI_API_BASE",
        "ZAI_API_URL",
        "MINIMAX_API_KEY",
        "MINIMAX_CODING_API_KEY",
        "MINIMAX_API_TOKEN",
        "MINIMAX_BASE_URL",
        "MINIMAX_API_BASE",
        "MINIMAX_API_URL",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GEMINI_BASE_URL",
        "GOOGLE_BASE_URL",
        "GEMINI_API_BASE",
        "GEMINI_API_URL",
        "CURSOR_API_KEY",
        "CURSOR_BASE_URL",
        "CURSOR_API_BASE",
        "CURSOR_API_URL",
        "META_API_KEY",
        "META_BASE_URL",
        "META_API_BASE",
        "META_API_URL",
        "OPENROUTER_API_KEY",
        "OPENROUTER_BASE_URL",
        "OPENROUTER_API_URL",
        "OPENAI_BASE_URL",
        "OPENAI_API_BASE",
        "OPENAI_API_URL",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
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
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
    ] {
        assert!(
            names.contains(name),
            "account env registry must include {name}"
        );
    }
}

#[test]
fn account_env_registry_has_no_duplicate_names() {
    let names: Vec<_> = account_env_names().collect();
    let unique: std::collections::BTreeSet<_> = names.iter().copied().collect();
    assert_eq!(
        names.len(),
        unique.len(),
        "account env names must be unique"
    );
}
