// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;

fn instance_config(instances: &[(&str, &str, &str)]) -> CapsuleConfig {
    CapsuleConfig {
        workdir: "/workspace".to_owned(),
        instances: instances
            .iter()
            .map(|(id, _, _)| (*id).to_owned())
            .collect(),
        agents: instances
            .iter()
            .map(|(id, _, slug)| ((*id).to_owned(), (*slug).to_owned()))
            .collect(),
        auth_modes: instances
            .iter()
            .map(|(id, mode, _)| ((*id).to_owned(), (*mode).to_owned()))
            .collect(),
        ..CapsuleConfig::default()
    }
}

fn v2_credentials(value: serde_json::Value) -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_value(value).expect("v2 fixture must decode")
}

#[test]
fn auth_modes_are_complete_bounded_and_allowlisted() {
    let valid = instance_config(&[("codex-work", "api_key", "codex")]);
    validate(&valid).unwrap();

    let mut invalid = valid.clone();
    invalid
        .auth_modes
        .insert("codex-work".to_owned(), "private-mode".to_owned());
    assert!(validate(&invalid).is_err());
    invalid.auth_modes = BTreeMap::from([("claude-work".to_owned(), "sync".to_owned())]);
    assert!(validate(&invalid).is_err());
}

#[test]
fn protected_credentials_reject_profile_mode_and_arbitrary_environment() {
    let mut config = instance_config(&[("claude-work", "sync", "claude")]);
    let credentials = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-1",
                "env": {"ANTHROPIC_API_KEY": "fixture"},
            },
        },
    }));
    validate_agent_credentials(&config, &credentials).unwrap_err();
    config
        .auth_modes
        .insert("claude-work".into(), "api_key".into());
    validate_agent_credentials(&config, &credentials).unwrap();
    let invalid = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-1",
                "env": {"LD_PRELOAD": "/evil"},
            },
        },
    }));
    validate_agent_credentials(&config, &invalid).unwrap_err();
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
                    "account_id": "acc-1",
                    "env": {},
                },
            },
        }));
        assert!(validate_agent_credentials(&config, &empty).is_err());
    }
}

#[test]
fn v2_envelope_is_accepted() {
    let credentials = parse_agent_credentials(
        serde_json::json!({
            "schema_version": 2,
            "instances": {
                "claude-work": {
                    "agent": "claude",
                    "account_id": "acc-1",
                    "env": {"ANTHROPIC_API_KEY": "work-secret"},
                },
            },
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert_eq!(credentials.schema_version(), 2);
    assert_eq!(
        credentials
            .for_instance("claude-work")
            .and_then(|env| env.get("ANTHROPIC_API_KEY"))
            .map(String::as_str),
        Some("work-secret")
    );
    assert!(credentials.for_instance("claude-unknown").is_none());
}

#[test]
fn non_v2_envelopes_reject_with_explicit_upgrade_error() {
    let v1_shape = serde_json::json!({
        "claude": {"ANTHROPIC_API_KEY": "v1-agent-keyed-secret"},
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
        v1_shape.to_string(),
        missing_version.to_string(),
        wrong_version.to_string(),
        "not json at all".to_owned(),
    ] {
        let error = parse_agent_credentials(raw.as_bytes()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        let message = error.to_string();
        assert!(
            message.contains("v2 envelope"),
            "reject must name the expected v2 envelope: {message}"
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
