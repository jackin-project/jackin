// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for protocol types.
use super::*;

#[test]
fn label_round_trips_through_from_label() {
    for provider in Provider::ALL {
        assert_eq!(Provider::from_label(provider.label()), Some(provider));
    }
    assert_eq!(Provider::from_label("Gemini"), None);
}

#[test]
fn capsule_config_accessors_are_keyed_by_instance_config_id() {
    let config = CapsuleConfig {
        instances: vec!["claude-work".to_owned(), "claude-personal".to_owned()],
        models: BTreeMap::from([("claude-work".to_owned(), "opus-4-6".to_owned())]),
        auth_modes: BTreeMap::from([
            ("claude-work".to_owned(), "api_key".to_owned()),
            ("claude-personal".to_owned(), "sync".to_owned()),
        ]),
        ..CapsuleConfig::default()
    };
    assert_eq!(
        config.supported_instances(),
        vec!["claude-work".to_owned(), "claude-personal".to_owned()]
    );
    assert_eq!(config.model_for_instance("claude-work"), Some("opus-4-6"));
    assert_eq!(config.model_for_instance("claude-personal"), None);
    assert_eq!(
        config.auth_mode_for_instance("claude-personal"),
        Some("sync")
    );
    assert_eq!(config.auth_mode_for_instance("codex-work"), None);
}
