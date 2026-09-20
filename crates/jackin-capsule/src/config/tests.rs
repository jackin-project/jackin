// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;

fn instance_config(instances: &[(&str, &str, &str)]) -> CapsuleConfig {
    let mut config = CapsuleConfig {
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
        accounts: instances
            .iter()
            .map(|(id, _, _)| {
                let account = match *id {
                    "claude-work" => "acc-work",
                    "claude-personal" => "acc-personal",
                    "codex-work" => "acc-codex",
                    _ => id,
                };
                ((*id).to_owned(), account.to_owned())
            })
            .collect(),
        ..CapsuleConfig::default()
    };
    for (index, (id, _, _)) in instances.iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        config
            .instance_home_dirs
            .insert((*id).to_owned(), format!("/home/agent/.slot-{index}"));
        config
            .instance_forwarded_dirs
            .insert((*id).to_owned(), format!("/jackin/slot-{index}"));
        config.instance_credential_files.insert(
            (*id).to_owned(),
            jackin_protocol::account_credentials_container_path(id),
        );
        config.instance_mount_paths.insert(
            (*id).to_owned(),
            vec![
                format!("/home/agent/.slot-{index}"),
                format!("/jackin/slot-{index}"),
            ],
        );
        config.instance_identities.insert(
            (*id).to_owned(),
            jackin_protocol::SessionIdentity {
                uid: 2_000 + index,
                gid: 2_000 + index,
            },
        );
    }
    config.shell_identity = Some(jackin_protocol::SessionIdentity {
        uid: 2_000 + u32::try_from(instances.len()).unwrap_or(u32::MAX),
        gid: 2_000 + u32::try_from(instances.len()).unwrap_or(u32::MAX),
    });
    config
}

fn v2_credentials(value: serde_json::Value) -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_value(value).expect("v2 fixture must decode")
}

#[test]
fn workdir_boundary_rejects_root_ancestors_and_noncanonical_aliases() {
    for workdir in ["/", "/home", "/jackin", "/home/agent", "/workspace/../"] {
        let mut config = instance_config(&[]);
        config.workdir = workdir.to_owned();
        let error = validate(&config).unwrap_err();
        assert!(
            error.to_string().contains("protected root"),
            "unexpected rejection for {workdir}: {error}"
        );
    }
}

#[test]
fn workdir_boundary_preserves_workspace_and_rejects_private_mount_ancestors() {
    let valid = instance_config(&[("codex-work", "api_key", "codex")]);
    validate(&valid).unwrap();

    let mut invalid = valid;
    invalid.workdir = "/workspace".to_owned();
    invalid.instance_mount_paths.insert(
        "codex-work".to_owned(),
        vec![
            "/home/agent/.slot-0".to_owned(),
            "/jackin/slot-0".to_owned(),
            "/workspace/private-slot".to_owned(),
        ],
    );
    let error = validate(&invalid).unwrap_err();
    assert!(
        error.to_string().contains("protected mount destination"),
        "unexpected rejection: {error}"
    );
}

fn isolated_entry(dst: &str) -> jackin_protocol::IsolatedWorktree {
    jackin_protocol::IsolatedWorktree {
        dst: dst.to_owned(),
        readonly: false,
        worktree: false,
        shared: true,
    }
}

#[test]
fn isolated_worktrees_reject_protected_and_private_destinations() {
    let mut valid = instance_config(&[("codex-work", "api_key", "codex")]);
    valid.isolated_worktrees = vec![
        isolated_entry("/workspace/extra"),
        isolated_entry("/jackin/work/jackin"),
    ];
    validate(&valid).unwrap();

    for hostile in [
        "",
        "/home/agent/evil",
        "/jackin/run/evil",
        "/jackin/state/evil",
        "/jackin/runtime/evil",
        "/jackin/host/evil",
        "/home/agent/.slot-0",
        "relative/path",
        "/workspace/../home/agent",
        "/",
    ] {
        let mut invalid = valid.clone();
        invalid.isolated_worktrees = vec![isolated_entry(hostile)];
        assert!(
            validate(&invalid).is_err(),
            "hostile destination must be rejected: {hostile:?}"
        );
    }
}

#[test]
fn instance_boundary_rejects_protected_root_aliases() {
    let mut invalid_home = instance_config(&[("codex-work", "api_key", "codex")]);
    invalid_home
        .instance_home_dirs
        .insert("codex-work".to_owned(), "/home/agent/".to_owned());
    assert!(validate(&invalid_home).is_err());

    let mut invalid_forwarded = instance_config(&[("codex-work", "api_key", "codex")]);
    invalid_forwarded
        .instance_forwarded_dirs
        .insert("codex-work".to_owned(), "/jackin/".to_owned());
    assert!(validate(&invalid_forwarded).is_err());

    let mut invalid_mount = instance_config(&[("codex-work", "api_key", "codex")]);
    invalid_mount.instance_mount_paths.insert(
        "codex-work".to_owned(),
        vec!["/home/agent/".to_owned(), "/jackin/slot-0".to_owned()],
    );
    assert!(validate(&invalid_mount).is_err());
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
                "account_id": "acc-work",
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
                "account_id": "acc-work",
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
