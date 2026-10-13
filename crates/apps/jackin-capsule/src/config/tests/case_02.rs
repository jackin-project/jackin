// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn protected_credentials_preserve_claude_routed_auth_token_and_reject_foreign_key() {
    let mut config = instance_config(&[("claude-work", "api_key", "claude")]);
    config
        .credential_provider_surfaces
        .insert("claude-work".to_owned(), "zai".to_owned());
    let valid = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "selected-zai-token",
                    "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic"
                },
            },
        },
    }));
    validate_agent_credentials(&config, &valid).unwrap();

    let foreign = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "selected-zai-token",
                    "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic",
                    "OPENAI_API_KEY": "foreign-codex-sentinel"
                },
            },
        },
    }));
    let error = validate_agent_credentials(&config, &foreign).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!error.to_string().contains("foreign-codex-sentinel"));
}

#[test]
fn protected_credentials_required_for_secret_auth_modes() {
    for mode in ["api_key", "oauth_token"] {
        let config = instance_config(&[("claude-work", mode, "claude")]);
        assert!(
            validate_agent_credentials(&config, &jackin_protocol::AgentCredentialEnv::default())
                .is_err()
        );
        let empty = v2_credentials(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "claude-work": {
                    "agent": "claude",
                    "account_id": "acc-work",
                    "env": {},
                },
            },
        }));
        assert!(validate_agent_credentials(&config, &empty).is_err());
    }
}

#[test]
fn single_instance_staged_credential_is_accepted() {
    let staged = parse_staged_credential(
        serde_json::json!({
            "schema_version": 1,
            "instance": "claude-work",
            "credential": {
                "agent": "claude",
                "account_id": "acc-1",
                "env": {"ANTHROPIC_API_KEY": "work-secret"},
            },
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert_eq!(staged.schema_version, 1);
    assert_eq!(
        staged
            .credential
            .env
            .get("ANTHROPIC_API_KEY")
            .map(String::as_str),
        Some("work-secret")
    );
    assert_eq!(staged.instance, "claude-work");
}

#[test]
fn invalid_staged_credentials_reject_with_explicit_upgrade_error() {
    let old_envelope = serde_json::json!({
        "schema_version": 2,
        "instances": {},
    });
    let missing_version = serde_json::json!({
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-1",
                "env": {"ANTHROPIC_API_KEY": "unversioned-secret"},
            },
        },
    });
    let wrong_version = serde_json::json!({
        "schema_version": 1,
        "instances": {},
    });
    for raw in [
        old_envelope.to_string(),
        missing_version.to_string(),
        wrong_version.to_string(),
        "not json at all".to_owned(),
    ] {
        let error = parse_staged_credential(raw.as_bytes()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        let message = error.to_string();
        assert!(
            message.contains("single-instance") || message.contains("staged"),
            "reject must name the expected staged format: {message}"
        );
        assert!(
            message.contains("restart"),
            "reject must demand a restart/upgrade (H03): {message}"
        );
        assert!(
            !message.contains("secret"),
            "reject must never echo credential material: {message}"
        );
    }
}

#[test]
fn several_instances_may_share_one_agent_with_isolated_env() {
    let config = instance_config(&[
        ("claude-work", "api_key", "claude"),
        ("claude-personal", "api_key", "claude"),
    ]);
    validate(&config).unwrap();
    let credentials = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {"ANTHROPIC_API_KEY": "work-secret"},
            },
            "claude-personal": {
                "agent": "claude",
                "account_id": "acc-personal",
                "env": {"ANTHROPIC_API_KEY": "personal-secret"},
            },
        },
    }));
    validate_agent_credentials(&config, &credentials).unwrap();
    for (instance, own, other) in [
        ("claude-work", "work-secret", "personal-secret"),
        ("claude-personal", "personal-secret", "work-secret"),
    ] {
        let env = credentials
            .for_instance(instance)
            .expect("admitted instance resolves");
        assert_eq!(env.get("ANTHROPIC_API_KEY").map(String::as_str), Some(own));
        assert!(
            !env.values().any(|value| value == other),
            "sibling instance env must not leak across the shared agent"
        );
    }
    // Per-instance requirement: one empty sibling fails even when the other is
    // fully provisioned.
    let half_empty = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {"ANTHROPIC_API_KEY": "work-secret"},
            },
            "claude-personal": {
                "agent": "claude",
                "account_id": "acc-personal",
                "env": {},
            },
        },
    }));
    validate_agent_credentials(&config, &half_empty).unwrap_err();
}

#[test]
fn protected_credentials_must_match_admitted_agent_and_account() {
    let config = instance_config(&[("claude-work", "api_key", "claude")]);
    let swapped_agent = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "codex",
                "account_id": "acc-work",
                "env": {"ANTHROPIC_API_KEY": "fixture"},
            },
        },
    }));
    assert!(validate_agent_credentials(&config, &swapped_agent).is_err());

    let swapped_account = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-personal",
                "env": {"ANTHROPIC_API_KEY": "fixture"},
            },
        },
    }));
    assert!(validate_agent_credentials(&config, &swapped_account).is_err());
}

#[test]
fn overlapping_private_mounts_are_rejected() {
    let mut config = instance_config(&[
        ("slot-a", "api_key", "claude"),
        ("slot-b", "api_key", "codex"),
    ]);
    config
        .instance_forwarded_dirs
        .insert("slot-a".into(), "/jackin/shared".into());
    config
        .instance_forwarded_dirs
        .insert("slot-b".into(), "/jackin/shared/child".into());
    config.instance_mount_paths.insert(
        "slot-a".into(),
        vec!["/home/agent/.slot-0".into(), "/jackin/shared".into()],
    );
    config.instance_mount_paths.insert(
        "slot-b".into(),
        vec!["/home/agent/.slot-1".into(), "/jackin/shared/child".into()],
    );
    assert!(validate(&config).is_err());
}

#[test]
fn xdg_agent_allows_its_paired_config_root_without_widening_home_access() {
    let mut config = instance_config(&[("amp", "sync", "amp")]);
    config
        .instance_home_dirs
        .insert("amp".into(), "/home/agent/.local/share".into());
    config
        .instance_forwarded_dirs
        .insert("amp".into(), "/jackin/amp".into());
    config.instance_mount_paths.insert(
        "amp".into(),
        vec![
            "/home/agent/.local/share/amp".into(),
            "/home/agent/.config/amp".into(),
            "/home/agent/.cache/amp".into(),
            "/jackin/amp".into(),
        ],
    );
    config
        .instance_cache_dirs
        .insert("amp".into(), "/home/agent/.cache/amp".into());

    validate(&config).unwrap();

    config.instance_mount_paths.insert(
        "amp".into(),
        vec![
            "/home/agent/.local/share/amp".into(),
            "/home/agent/.config/opencode".into(),
            "/home/agent/.cache/amp".into(),
            "/jackin/amp".into(),
        ],
    );
    assert!(validate(&config).is_err());
}

#[test]
fn unknown_instances_are_rejected() {
    let config = instance_config(&[("claude-work", "api_key", "claude")]);
    let credentials = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {"ANTHROPIC_API_KEY": "work-secret"},
            },
            "codex-stowaway": {
                "agent": "codex",
                "account_id": "acc-codex",
                "env": {"OPENAI_API_KEY": "stowaway-secret"},
            },
        },
    }));
    validate_agent_credentials(&config, &credentials).unwrap_err();
}
