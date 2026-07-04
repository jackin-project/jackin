// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `auth`.
use super::*;

#[test]
fn label_matches_design_spec() {
    assert_eq!(AuthKind::Claude.label(), "Claude Code");
    assert_eq!(AuthKind::Codex.label(), "Codex");
    assert_eq!(AuthKind::Amp.label(), "Amp");
    assert_eq!(AuthKind::Kimi.label(), "Kimi");
    assert_eq!(AuthKind::Opencode.label(), "OpenCode");
    assert_eq!(AuthKind::Github.label(), "GitHub CLI");
}

#[test]
fn auth_kind_order_lists_match_console_surfaces() {
    assert_eq!(
        AuthKind::WORKSPACE_PANEL_KINDS,
        &[
            AuthKind::Claude,
            AuthKind::Codex,
            AuthKind::Amp,
            AuthKind::Opencode,
            AuthKind::Grok,
            AuthKind::Github,
            AuthKind::Zai,
            AuthKind::Minimax,
        ],
    );
    assert_eq!(
        AuthKind::SETTINGS_KINDS,
        &[
            AuthKind::Claude,
            AuthKind::Codex,
            AuthKind::Amp,
            AuthKind::Kimi,
            AuthKind::Opencode,
            AuthKind::Grok,
            AuthKind::Github,
            AuthKind::Zai,
            AuthKind::Minimax,
        ],
    );
}

#[test]
fn github_supported_modes_are_sync_token_ignore() {
    let modes = AuthKind::Github.supported_modes();
    assert_eq!(modes, &[AuthMode::Sync, AuthMode::Token, AuthMode::Ignore]);
}

#[test]
fn claude_supported_modes_include_oauth_token() {
    assert!(
        AuthKind::Claude
            .supported_modes()
            .contains(&AuthMode::OAuthToken)
    );
}

#[test]
fn non_claude_agent_modes_exclude_oauth_token_and_token() {
    for kind in [
        AuthKind::Codex,
        AuthKind::Amp,
        AuthKind::Kimi,
        AuthKind::Opencode,
    ] {
        let modes = kind.supported_modes();
        assert!(!modes.contains(&AuthMode::OAuthToken));
        assert!(!modes.contains(&AuthMode::Token));
    }
}

#[test]
fn required_env_vars_match_auth_kind_table() {
    assert_eq!(
        AuthKind::Claude.required_env_var(AuthMode::ApiKey),
        Some(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME)
    );
    assert_eq!(
        AuthKind::Claude.required_env_var(AuthMode::OAuthToken),
        Some(jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
    );
    assert_eq!(
        AuthKind::Github.required_env_var(AuthMode::Token),
        Some(jackin_core::env_model::GH_TOKEN_ENV_NAME)
    );
    assert_eq!(
        AuthKind::Zai.required_env_var(AuthMode::ApiKey),
        Some(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
    );
    assert_eq!(AuthKind::Github.required_env_var(AuthMode::Sync), None);
}

#[test]
fn kimi_required_env_vars_match_runtime_table() {
    assert_eq!(
        AuthKind::Kimi.required_env_var(AuthMode::ApiKey),
        Some(jackin_core::env_model::KIMI_CODE_API_KEY_ENV_NAME)
    );
    assert_eq!(AuthKind::Kimi.required_env_var(AuthMode::Sync), None);
    assert_eq!(AuthKind::Kimi.required_env_var(AuthMode::Ignore), None);
}

#[test]
fn minimax_required_env_var_matches_constant() {
    assert_eq!(
        AuthKind::Minimax.required_env_var(AuthMode::ApiKey),
        Some(jackin_core::env_model::MINIMAX_API_KEY_ENV_NAME)
    );
    assert_eq!(AuthKind::Minimax.required_env_var(AuthMode::Sync), None);
}

#[test]
fn minimax_supported_modes_are_api_key_and_ignore() {
    assert_eq!(
        AuthKind::Minimax.supported_modes(),
        &[AuthMode::ApiKey, AuthMode::Ignore]
    );
}

#[test]
fn token_generation_gate_is_claude_oauth_only() {
    assert!(can_generate_claude_oauth_token(
        AuthKind::Claude,
        Some(AuthMode::OAuthToken),
    ));
    assert!(!can_generate_claude_oauth_token(
        AuthKind::Claude,
        Some(AuthMode::ApiKey),
    ));
    assert!(!can_generate_claude_oauth_token(
        AuthKind::Github,
        Some(AuthMode::Token),
    ));
    assert!(!can_generate_claude_oauth_token(AuthKind::Claude, None));
}

#[test]
fn credential_requirement_tracks_required_env_var() {
    assert!(auth_mode_requires_credential(
        AuthKind::Claude,
        AuthMode::ApiKey,
    ));
    assert!(auth_mode_requires_credential(
        AuthKind::Github,
        AuthMode::Token,
    ));
    assert!(!auth_mode_requires_credential(
        AuthKind::Claude,
        AuthMode::Sync,
    ));
    assert!(!auth_mode_requires_credential(
        AuthKind::Github,
        AuthMode::Ignore,
    ));
}

#[test]
fn source_folder_gate_is_sync_agent_only() {
    for kind in [
        AuthKind::Claude,
        AuthKind::Codex,
        AuthKind::Amp,
        AuthKind::Kimi,
        AuthKind::Opencode,
    ] {
        assert!(auth_mode_supports_source_folder(kind, AuthMode::Sync));
        assert!(!auth_mode_supports_source_folder(kind, AuthMode::ApiKey));
        assert!(!auth_mode_supports_source_folder(kind, AuthMode::Ignore));
    }

    assert!(!auth_mode_supports_source_folder(
        AuthKind::Github,
        AuthMode::Sync,
    ));
    assert!(!auth_mode_supports_source_folder(
        AuthKind::Zai,
        AuthMode::ApiKey,
    ));
    assert!(!auth_mode_supports_source_folder(
        AuthKind::Minimax,
        AuthMode::ApiKey,
    ));
}
