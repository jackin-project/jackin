// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn registered_account_declaration_resolves_in_broker_service() {
    let mut config = jackin_config::AppConfig::default();
    config
        .env
        .insert("OPENAI_API_KEY".into(), "fixture-account-key".into());
    let entry = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
        .iter()
        .find(|entry| entry.name == "OPENAI_API_KEY")
        .copied()
        .unwrap();
    let resolution = ServiceSecretSource
        .resolve_secret(&config, None, None, entry)
        .unwrap();
    assert!(
        matches!(resolution.outcome, ProviderCredentialSecretOutcome::Resolved(ref secret) if secret == "fixture-account-key")
    );
}

#[test]
fn unattended_broker_never_resolves_op_or_on_demand_declarations() {
    let op_ref = jackin_core::EnvValue::OpRef(jackin_core::OpRef {
        op: "op://vault/item/field".to_owned(),
        path: "Vault/Item/Field".to_owned(),
        account: None,
        on_demand: false,
    });
    let on_demand = jackin_core::EnvValue::Extended(jackin_core::Extended {
        value: "$ANTHROPIC_API_KEY".to_owned(),
        on_demand: true,
    });
    let plain = jackin_core::EnvValue::from("fixture-only-token");

    assert!(broker_secret_requires_interaction(&op_ref));
    assert!(broker_secret_requires_interaction(&on_demand));
    assert!(!broker_secret_requires_interaction(&plain));
}

#[test]
fn local_only_flag_selects_the_discovery_free_service_mode() {
    assert!(local_only_requested(&[
        "jackin-usage-broker".to_owned(),
        "--data-dir".to_owned(),
        "/tmp/data".to_owned(),
        "--local-only".to_owned(),
    ]));
    assert!(!local_only_requested(&[
        "jackin-usage-broker".to_owned(),
        "--config-root".to_owned(),
        "/tmp/config".to_owned(),
    ]));
}

fn prepare_auth_args(extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "jackin-usage-broker".to_owned(),
        "--prepare-auth".to_owned(),
        "--provider".to_owned(),
        "claude".to_owned(),
    ];
    args.extend(extra.iter().map(|value| (*value).to_owned()));
    args
}

#[test]
fn explicit_auth_helper_requires_all_terminal_streams_before_keychain_read() {
    let args = prepare_auth_args(&[]);
    let (exit_code, json) = prepare_auth_with(&args, false, |_, _| {
        panic!("headless auth helper must not read Keychain")
    });

    assert_eq!(exit_code, 2);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["error"]["code"], "interaction_required");
}

#[test]
fn explicit_auth_helper_uses_operator_policy_and_never_returns_payload() {
    let args = prepare_auth_args(&[]);
    let (exit_code, json) = prepare_auth_with(&args, true, |service, policy| {
        assert_eq!(service, jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE);
        assert_eq!(
            policy,
            jackin_usage_provider_claude::ClaudeKeychainInteractionPolicy::OperatorInitiated
        );
        AuthReadOutcome::Payload("secret-canary".to_owned())
    });

    assert_eq!(exit_code, 0);
    assert!(!json.contains("secret-canary"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["result"], "auth_prepared");
}

#[test]
fn explicit_auth_helper_maps_missing_consent_and_denial_to_stable_codes() {
    let args = prepare_auth_args(&["--keychain-service", "Claude custom"]);
    for (read, expected) in [
        (AuthReadOutcome::Missing, "auth_missing"),
        (AuthReadOutcome::ConsentRequired, "interaction_required"),
        (AuthReadOutcome::Denied, "auth_denied"),
    ] {
        let (exit_code, json) = prepare_auth_with(&args, true, |service, _| {
            assert_eq!(service, "Claude custom");
            read
        });
        assert_eq!(exit_code, 2);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["error"]["code"], expected);
    }
}

#[test]
fn prepare_auth_mode_is_detected_before_detach() {
    assert!(prepare_auth_requested(&prepare_auth_args(&[])));
    assert!(!prepare_auth_requested(&[
        "jackin-usage-broker".to_owned(),
        "--local-only".to_owned(),
    ]));
}
