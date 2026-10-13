// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn account_credentials_are_scoped_to_selected_instance_and_mode() {
    let credentials = v2_credentials_fixture();
    let hostile_passthrough = vec![("ANTHROPIC_API_KEY".into(), "wrong-secret".into())];
    for mode in ["sync", "ignore"] {
        let mut cmd = build_agent_command(&spawn_spec(
            "claude",
            "claude-work",
            Some(mode),
            &hostile_passthrough,
        ));
        apply_account_env(&mut cmd, "claude-work", Some(mode), None, &credentials);
        assert!(cmd.get_env("ANTHROPIC_API_KEY").is_none());
    }
    let mut cmd = build_agent_command(&spawn_spec(
        "claude",
        "claude-work",
        Some("api_key"),
        &hostile_passthrough,
    ));
    apply_account_env(
        &mut cmd,
        "claude-work",
        Some("api_key"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        cmd.get_env("ANTHROPIC_API_KEY").and_then(|v| v.to_str()),
        Some("work-secret")
    );
    assert!(cmd.get_env("OPENAI_API_KEY").is_none());
    // Same agent, sibling instance: only its own env lands, never the other
    // claude instance's secret.
    let mut cmd = build_agent_command(&spawn_spec(
        "claude",
        "claude-work",
        Some("api_key"),
        &hostile_passthrough,
    ));
    apply_account_env(
        &mut cmd,
        "claude-personal",
        Some("api_key"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        cmd.get_env("ANTHROPIC_API_KEY").and_then(|v| v.to_str()),
        Some("personal-secret")
    );
    let shell = build_shell_command(
        &hostile_passthrough,
        Path::new("/workspace"),
        "test",
        jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    );
    assert!(shell.get_env("ANTHROPIC_API_KEY").is_none());
}

#[test]
fn account_env_injection_is_bounded_by_agent_provider_and_auth_family() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "codex-routed": {
                    "agent": "codex",
                    "account_id": "acc-zai",
                    "env": {
                        "KIMI_API_KEY": "selected-routed-key",
                        "OPENAI_BASE_URL": "https://api.kimi.example/v1",
                        "CLAUDE_CODE_OAUTH_TOKEN": "foreign-claude-sentinel",
                        "GEMINI_API_KEY": "foreign-google-sentinel"
                    },
                },
                "opencode-routed": {
                    "agent": "opencode",
                    "account_id": "acc-anthropic",
                    "env": {
                        "ANTHROPIC_API_KEY": "selected-opencode-key",
                        "CLAUDE_CODE_OAUTH_TOKEN": "foreign-claude-sentinel"
                    },
                },
                "claude-oauth": {
                    "agent": "claude",
                    "account_id": "acc-claude",
                    "env": {
                        "CLAUDE_CODE_OAUTH_TOKEN": "selected-oauth-token",
                        "ANTHROPIC_BASE_URL": "https://anthropic.example",
                        "OPENAI_API_KEY": "foreign-codex-sentinel"
                    },
                },
                "claude-routed": {
                    "agent": "claude",
                    "account_id": "acc-zai",
                    "env": {
                        "ANTHROPIC_AUTH_TOKEN": "selected-zai-token",
                        "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic",
                        "OPENAI_API_KEY": "foreign-codex-sentinel"
                    },
                },
            },
        }))
        .expect("credential fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();

    let mut codex = build_agent_command(&spawn_spec(
        "codex",
        "codex-routed",
        Some("api_key"),
        &empty,
    ));
    apply_account_env(
        &mut codex,
        "codex-routed",
        Some("api_key"),
        Some("kimi"),
        &credentials,
    );
    assert_eq!(
        codex
            .get_env("KIMI_API_KEY")
            .and_then(|value| value.to_str()),
        Some("selected-routed-key")
    );
    assert_eq!(
        codex
            .get_env("OPENAI_BASE_URL")
            .and_then(|value| value.to_str()),
        Some("https://api.kimi.example/v1")
    );
    assert!(codex.get_env("CLAUDE_CODE_OAUTH_TOKEN").is_none());
    assert!(codex.get_env("GEMINI_API_KEY").is_none());

    let mut opencode = build_agent_command(&spawn_spec(
        "opencode",
        "opencode-routed",
        Some("api_key"),
        &empty,
    ));
    apply_account_env(
        &mut opencode,
        "opencode-routed",
        Some("api_key"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        opencode
            .get_env("ANTHROPIC_API_KEY")
            .and_then(|value| value.to_str()),
        Some("selected-opencode-key")
    );
    assert!(opencode.get_env("CLAUDE_CODE_OAUTH_TOKEN").is_none());
    assert!(opencode.get_env("OPENAI_API_KEY").is_none());

    let mut claude = build_agent_command(&spawn_spec(
        "claude",
        "claude-oauth",
        Some("oauth_token"),
        &empty,
    ));
    apply_account_env(
        &mut claude,
        "claude-oauth",
        Some("oauth_token"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        claude
            .get_env("CLAUDE_CODE_OAUTH_TOKEN")
            .and_then(|value| value.to_str()),
        Some("selected-oauth-token")
    );
    assert_eq!(
        claude
            .get_env("ANTHROPIC_BASE_URL")
            .and_then(|value| value.to_str()),
        Some("https://anthropic.example")
    );
    assert!(claude.get_env("OPENAI_API_KEY").is_none());

    let mut routed_claude = build_agent_command(&spawn_spec(
        "claude",
        "claude-routed",
        Some("api_key"),
        &empty,
    ));
    apply_account_env(
        &mut routed_claude,
        "claude-routed",
        Some("api_key"),
        Some("zai"),
        &credentials,
    );
    assert_eq!(
        routed_claude
            .get_env("ANTHROPIC_AUTH_TOKEN")
            .and_then(|value| value.to_str()),
        Some("selected-zai-token")
    );
    assert_eq!(
        routed_claude
            .get_env("ANTHROPIC_BASE_URL")
            .and_then(|value| value.to_str()),
        Some("https://api.z.ai/api/anthropic")
    );
    assert!(routed_claude.get_env("OPENAI_API_KEY").is_none());
}

#[test]
fn moonshot_opencode_credential_requires_selected_surface_and_rejects_foreign_key() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "opencode-kimi": {
                    "agent": "opencode",
                    "account_id": "acc-kimi",
                    "env": {
                        "MOONSHOT_API_KEY": "selected-moonshot-key",
                        "OPENAI_API_KEY": "foreign-openai-sentinel"
                    },
                },
            },
        }))
        .expect("credential fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();

    let mut selected = build_agent_command(&spawn_spec(
        "opencode",
        "opencode-kimi",
        Some("api_key"),
        &empty,
    ));
    apply_account_env(
        &mut selected,
        "opencode-kimi",
        Some("api_key"),
        Some("kimi"),
        &credentials,
    );
    assert_eq!(
        selected
            .get_env(jackin_core::MOONSHOT_API_KEY_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("selected-moonshot-key")
    );
    assert!(selected.get_env("OPENAI_API_KEY").is_none());

    let mut unselected = build_agent_command(&spawn_spec(
        "opencode",
        "opencode-kimi",
        Some("api_key"),
        &empty,
    ));
    apply_account_env(
        &mut unselected,
        "opencode-kimi",
        Some("api_key"),
        None,
        &credentials,
    );
    assert!(unselected.get_env("MOONSHOT_API_KEY").is_none());
}

#[test]
fn routed_claude_credentials_require_selected_surface() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "claude-routed": {
                    "agent": "claude",
                    "account_id": "acc-zai",
                    "env": {
                        "ANTHROPIC_AUTH_TOKEN": "selected-zai-token",
                        "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic"
                    },
                },
            },
        }))
        .expect("credential fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();
    let mut selected = build_agent_command(&spawn_spec(
        "claude",
        "claude-routed",
        Some("api_key"),
        &empty,
    ));
    apply_account_env(
        &mut selected,
        "claude-routed",
        Some("api_key"),
        Some("zai"),
        &credentials,
    );
    assert_eq!(
        selected
            .get_env("ANTHROPIC_AUTH_TOKEN")
            .and_then(|value| value.to_str()),
        Some("selected-zai-token")
    );

    let mut unselected = build_agent_command(&spawn_spec(
        "claude",
        "claude-routed",
        Some("api_key"),
        &empty,
    ));
    apply_account_env(
        &mut unselected,
        "claude-routed",
        Some("api_key"),
        None,
        &credentials,
    );
    assert!(
        unselected.get_env("ANTHROPIC_AUTH_TOKEN").is_none(),
        "routed Claude credentials must not inject without a selected provider surface"
    );
}
