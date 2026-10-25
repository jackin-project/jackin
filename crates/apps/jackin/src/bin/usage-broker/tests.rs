// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::sync::Mutex;

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
        "--data-dir".to_owned(),
        "/tmp/data".to_owned(),
        "--config-root".to_owned(),
        "/tmp/config".to_owned(),
        "--operator-home".to_owned(),
        "/tmp/home".to_owned(),
        "--build-id".to_owned(),
        "test-build".to_owned(),
    ];
    args.extend(extra.iter().map(|value| (*value).to_owned()));
    args
}

fn test_ready() -> UsageBrokerForegroundReady {
    UsageBrokerForegroundReady {
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            surface_id: "claude".to_owned(),
            account_id: "opaque-broker-capability".to_owned(),
        },
        binding_scope: "claude_keychain_service".to_owned(),
    }
}

#[test]
fn explicit_auth_requires_all_terminal_streams_before_bootstrap() {
    let args = prepare_auth_args(&[]);
    let result = prepare_auth_with(
        &args,
        || false,
        |_, _| panic!("headless auth must not claim a broker or read Keychain"),
        |_| panic!("headless auth must not report service readiness"),
    );

    let (exit_code, json) = result.unwrap_err();
    assert_eq!(exit_code, 2);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["error"]["code"], "interaction_required");
    assert!(value["error"].get("diagnostic").is_none());
}

#[test]
fn explicit_auth_holds_foreground_service_and_reports_only_safe_readiness() {
    let args = prepare_auth_args(&["--keychain-service", "Claude custom"]);
    let ready = Arc::new(Mutex::new(None));
    let ready_capture = Arc::clone(&ready);
    let result = prepare_auth_with(
        &args,
        || true,
        move |request, on_ready| {
            assert_eq!(request.keychain_service, "Claude custom");
            let debug = format!("{request:?}");
            assert!(debug.contains("REDACTED"));
            assert!(!debug.contains("Claude custom"));
            assert_eq!(request.data_dir, PathBuf::from("/tmp/data"));
            assert_eq!(request.config_root, PathBuf::from("/tmp/config"));
            assert_eq!(request.operator_home, PathBuf::from("/tmp/home"));
            assert_eq!(request.build_id, "test-build");
            assert_eq!(request.socket_alias_root, None);
            on_ready(test_ready());
            Ok(ForegroundBootstrapOutcome::Acquired)
        },
        move |ready| *ready_capture.lock().unwrap() = Some(service_ready_json(&ready)),
    );

    result.unwrap();
    let json = ready.lock().unwrap().clone().unwrap();
    assert!(!json.contains("Claude custom"));
    assert!(!json.contains("secret"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["result"], "service_ready");
    assert_eq!(value["source"]["scope"], "claude_keychain_service");
    assert_eq!(value["source"]["account_id"], "opaque-broker-capability");
}

#[test]
fn foreground_bootstrap_maps_safe_outcomes_to_stable_codes() {
    let args = prepare_auth_args(&[]);
    assert_eq!(
        ForegroundBootstrapOutcome::from(ClaudeCredentialBootstrapOutcome::AccessUnavailable),
        ForegroundBootstrapOutcome::AccessUnavailable
    );
    let malformed = jackin_usage_provider_claude::diagnose_claude_profile_payload(
        br#"{"claudeAiOauth":{"accessToken":"fixture-token"}}"#,
    );
    for (outcome, expected, expected_message) in [
        (ForegroundBootstrapOutcome::Missing, "auth_missing", None),
        (ForegroundBootstrapOutcome::Denied, "auth_denied", None),
        (
            ForegroundBootstrapOutcome::InteractionRequired,
            "interaction_required",
            Some(
                "authentication preparation requires stdin, stdout, and stderr attached to a terminal",
            ),
        ),
        (
            ForegroundBootstrapOutcome::AccessUnavailable,
            "keychain_access_unavailable",
            Some("Keychain credential access is unavailable during authentication preparation"),
        ),
        (
            ForegroundBootstrapOutcome::Malformed(malformed.clone()),
            "auth_malformed",
            None,
        ),
    ] {
        let result = prepare_auth_with(
            &args,
            || true,
            move |_, _| Ok(outcome),
            |_| panic!("failed bootstrap must not report readiness"),
        );
        let (exit_code, json) = result.unwrap_err();
        assert_eq!(exit_code, 2);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["error"]["code"], expected);
        if let Some(expected_message) = expected_message {
            assert_eq!(value["error"]["message"], expected_message);
        }
        assert_eq!(
            value["error"].get("diagnostic").is_some(),
            expected == "auth_malformed"
        );
    }
}

#[test]
fn malformed_auth_json_contains_only_bounded_diagnostic_facts() {
    let fixture = br#"{"claudeAiOauth":{"accessToken":"fixture-secret-token","subscriptionType":7},"oauthAccount":{"emailAddress":"fixture-private@example.test"},"unknownPrivateField":"fixture-unknown-secret"}"#;
    let diagnostic = jackin_usage_provider_claude::diagnose_claude_profile_payload(fixture);
    let (exit_code, json) = auth_malformed_error(diagnostic);

    assert_eq!(exit_code, 2);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["error"]["code"], "auth_malformed");
    assert_eq!(value["error"]["diagnostic"]["payload_bytes"], fixture.len());
    assert_eq!(
        value["error"]["diagnostic"]["access_token"]["camel_case"],
        "string"
    );
    assert_eq!(
        value["error"]["diagnostic"]["access_token"]["camel_case_nonempty"],
        true
    );
    assert!(!json.contains("fixture-secret-token"));
    assert!(!json.contains("fixture-private@example.test"));
    assert!(!json.contains("unknownPrivateField"));
    assert!(!json.contains("fixture-unknown-secret"));
}

#[test]
fn foreground_bootstrap_arguments_are_complete_and_strict() {
    let mut missing = prepare_auth_args(&[]);
    missing.truncate(missing.len() - 2);
    assert_eq!(parse_prepare_auth_args(&missing).unwrap_err().0, 3,);

    let mut wrong_provider = prepare_auth_args(&[]);
    let provider_index = wrong_provider
        .iter()
        .position(|arg| arg == "claude")
        .unwrap();
    wrong_provider[provider_index] = "codex".to_owned();
    assert_eq!(parse_prepare_auth_args(&wrong_provider).unwrap_err().0, 3);

    let unknown = prepare_auth_args(&["--unexpected"]);
    assert_eq!(parse_prepare_auth_args(&unknown).unwrap_err().0, 3);
}

#[test]
fn socket_alias_root_argument_is_optional_and_propagated_to_foreground_bootstrap() {
    let args = prepare_auth_args(&["--socket-alias-root", "/tmp/private-aliases"]);
    let request = parse_prepare_auth_args(&args).expect("valid custom alias root");
    assert_eq!(
        request.socket_alias_root,
        Some(PathBuf::from("/tmp/private-aliases"))
    );
}

#[test]
fn socket_alias_root_argument_rejects_missing_duplicate_and_invalid_values() {
    let base = vec!["jackin-usage-broker".to_owned()];
    let mut missing = base.clone();
    missing.push("--socket-alias-root".to_owned());
    parse_socket_alias_root_argument(&missing).unwrap_err();

    let mut duplicate = base.clone();
    duplicate.extend([
        "--socket-alias-root".to_owned(),
        "/tmp/first".to_owned(),
        "--socket-alias-root".to_owned(),
        "/tmp/second".to_owned(),
    ]);
    parse_socket_alias_root_argument(&duplicate).unwrap_err();

    for value in ["", "relative/path"] {
        let mut invalid = base.clone();
        invalid.extend(["--socket-alias-root".to_owned(), value.to_owned()]);
        parse_socket_alias_root_argument(&invalid).unwrap_err();
    }
}

#[test]
fn prepare_auth_mode_is_detected_only_as_the_foreground_command() {
    assert!(prepare_auth_requested(&prepare_auth_args(&[])));
    assert!(!prepare_auth_requested(&[
        "jackin-usage-broker".to_owned(),
        "--local-only".to_owned(),
        "--prepare-auth".to_owned(),
    ]));
}
