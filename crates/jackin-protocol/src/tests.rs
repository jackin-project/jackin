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
fn capsule_config_labels_and_accounts_are_keyed_by_instance_config_id() {
    let config = CapsuleConfig {
        instances: vec!["claude-work".to_owned(), "claude-personal".to_owned()],
        accounts: BTreeMap::from([
            ("claude-work".to_owned(), "work".to_owned()),
            ("claude-personal".to_owned(), "personal".to_owned()),
        ]),
        labels: BTreeMap::from([
            ("claude-work".to_owned(), "Claude · Work".to_owned()),
            ("claude-personal".to_owned(), "Personal Claude".to_owned()),
        ]),
        ..CapsuleConfig::default()
    };
    assert_eq!(config.account_for_instance("claude-work"), Some("work"));
    assert_eq!(
        config.account_for_instance("claude-personal"),
        Some("personal")
    );
    assert_eq!(config.account_for_instance("codex-work"), None);
    assert_eq!(
        config.label_for_instance("claude-work"),
        Some("Claude · Work")
    );
    assert_eq!(
        config.label_for_instance("claude-personal"),
        Some("Personal Claude")
    );
    assert_eq!(config.label_for_instance("codex-work"), None);
}

#[test]
fn resolve_instance_gate_rejects_unknown_and_ambiguous_targets() {
    let config = CapsuleConfig {
        instances: vec!["claude-work".to_owned(), "claude-personal".to_owned()],
        agents: BTreeMap::from([
            ("claude-work".to_owned(), "claude".to_owned()),
            ("claude-personal".to_owned(), "claude".to_owned()),
        ]),
        ..CapsuleConfig::default()
    };
    // Exact config IDs resolve even when the slug is ambiguous.
    assert_eq!(config.resolve_instance("claude-work"), Ok("claude-work"));
    assert_eq!(
        config.resolve_instance("claude-personal"),
        Ok("claude-personal")
    );
    // An ambiguous slug never silently picks an instance.
    config.resolve_instance("claude").unwrap_err();
    // Unknown targets error at the gate.
    config.resolve_instance("codex-work").unwrap_err();
    config.resolve_instance("codex").unwrap_err();

    let solo = CapsuleConfig {
        instances: vec!["codex-work".to_owned()],
        agents: BTreeMap::from([("codex-work".to_owned(), "codex".to_owned())]),
        ..CapsuleConfig::default()
    };
    // An unambiguous slug resolves to its sole instance.
    assert_eq!(solo.resolve_instance("codex"), Ok("codex-work"));

    let unmapped = CapsuleConfig {
        instances: vec!["ghost".to_owned()],
        ..CapsuleConfig::default()
    };
    // An admitted ID without a runtime mapping is not spawnable.
    unmapped.resolve_instance("ghost").unwrap_err();
}

#[test]
fn identity_wire_records_default_account_to_none_for_old_peers() {
    // Payloads written before account stamping (or by an older peer) omit
    // the new fields; they must decode with `None`, never fail.
    let info: control::SessionInfo = serde_json::from_value(serde_json::json!({
        "id": 1,
        "label": "Claude · Work",
        "agent": "claude-work",
        "state": "working",
        "active": true,
    }))
    .expect("old SessionInfo decodes");
    assert_eq!(info.account_id, None);

    let pane: control::PaneSnapshot = serde_json::from_value(serde_json::json!({
        "session_id": 1,
        "label": "Claude · Work",
        "agent": "claude-work",
        "state": "working",
    }))
    .expect("old PaneSnapshot decodes");
    assert_eq!(pane.account_id, None);

    let tab: control::TabSnapshot = serde_json::from_value(serde_json::json!({
        "label": "Claude · Work",
        "focused_pane": 1,
        "panes": [],
    }))
    .expect("old TabSnapshot decodes");
    assert_eq!(tab.instance, None);
    assert_eq!(tab.account_id, None);

    let record: control::SessionEventRecord = serde_json::from_value(serde_json::json!({
        "seq": 0,
        "session": 1,
        "agent": "claude-work",
        "state": "working",
        "kind": {"type": "subscribed"},
    }))
    .expect("old SessionEventRecord decodes");
    assert_eq!(record.account_id, None);

    let entry: control::AgentRegistryEntry = serde_json::from_value(serde_json::json!({
        "codename": "badger",
        "agent": "claude-work",
        "provider": "anthropic",
        "started_at": "2026-09-17T00:00:00Z",
        "exited_at": null,
        "status": "active",
    }))
    .expect("old AgentRegistryEntry decodes");
    assert_eq!(entry.account_id, None);
}

#[test]
fn capsule_config_accessors_are_keyed_by_instance_config_id() {
    let config = CapsuleConfig {
        instances: vec!["claude-work".to_owned(), "claude-personal".to_owned()],
        models: BTreeMap::from([("claude-work".to_owned(), "opus-4-6".to_owned())]),
        efforts: BTreeMap::from([("claude-work".to_owned(), "max".to_owned())]),
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
    assert_eq!(config.effort_for_instance("claude-work"), Some("max"));
    assert_eq!(config.effort_for_instance("claude-personal"), None);
    assert_eq!(
        config.auth_mode_for_instance("claude-personal"),
        Some("sync")
    );
    assert_eq!(config.auth_mode_for_instance("codex-work"), None);
}

#[test]
fn credential_provider_surface_is_independent_from_usage_authority() {
    let config = CapsuleConfig {
        instances: vec!["claude-work".to_owned()],
        credential_provider_surfaces: BTreeMap::from([(
            "claude-work".to_owned(),
            "zai".to_owned(),
        )]),
        ..CapsuleConfig::default()
    };

    assert_eq!(
        config.credential_provider_surface_for_instance("claude-work"),
        Some("zai")
    );
    assert_eq!(config.usage_capability_for_instance("claude-work"), None);
}
