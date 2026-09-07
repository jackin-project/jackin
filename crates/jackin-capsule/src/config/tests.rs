// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;

#[test]
fn auth_modes_are_complete_bounded_and_allowlisted() {
    let valid = CapsuleConfig {
        workdir: "/workspace".to_owned(),
        agents: vec!["codex".to_owned()],
        auth_modes: BTreeMap::from([("codex".to_owned(), "api_key".to_owned())]),
        ..CapsuleConfig::default()
    };
    validate(&valid).unwrap();

    let mut invalid = valid.clone();
    invalid
        .auth_modes
        .insert("codex".to_owned(), "private-mode".to_owned());
    assert!(validate(&invalid).is_err());
    invalid.auth_modes = BTreeMap::from([("claude".to_owned(), "sync".to_owned())]);
    assert!(validate(&invalid).is_err());
}

#[test]
fn protected_credentials_reject_profile_mode_and_arbitrary_environment() {
    let mut config = CapsuleConfig {
        agents: vec!["claude".into()],
        auth_modes: BTreeMap::from([("claude".into(), "sync".into())]),
        ..CapsuleConfig::default()
    };
    let credentials = jackin_protocol::AgentCredentialEnv::new(BTreeMap::from([(
        "claude".into(),
        BTreeMap::from([("ANTHROPIC_API_KEY".into(), "fixture".into())]),
    )]));
    validate_agent_credentials(&config, &credentials).unwrap_err();
    config.auth_modes.insert("claude".into(), "api_key".into());
    validate_agent_credentials(&config, &credentials).unwrap();
    let invalid = jackin_protocol::AgentCredentialEnv::new(BTreeMap::from([(
        "claude".into(),
        BTreeMap::from([("LD_PRELOAD".into(), "/evil".into())]),
    )]));
    validate_agent_credentials(&config, &invalid).unwrap_err();
}

#[test]
fn protected_credentials_required_for_secret_auth_modes() {
    for mode in ["api_key", "oauth_token"] {
        let config = CapsuleConfig {
            agents: vec!["claude".into()],
            auth_modes: BTreeMap::from([("claude".into(), mode.into())]),
            ..CapsuleConfig::default()
        };
        assert!(
            validate_agent_credentials(&config, &jackin_protocol::AgentCredentialEnv::default())
                .is_err()
        );
        let empty = jackin_protocol::AgentCredentialEnv::new(BTreeMap::from([(
            "claude".into(),
            BTreeMap::new(),
        )]));
        assert!(validate_agent_credentials(&config, &empty).is_err());
    }
}
