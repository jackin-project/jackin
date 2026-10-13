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

fn prepare_auth_args(extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "jackin-usage-broker".to_owned(),
        "--prepare-auth".to_owned(),
        "--provider".to_owned(),
        "claude".to_owned(),
        "--data-dir".to_owned(),
        "/tmp/usage-data".to_owned(),
        "--config-root".to_owned(),
        "/tmp/jackin-config".to_owned(),
        "--operator-home".to_owned(),
        "/tmp/operator-home".to_owned(),
        "--build-id".to_owned(),
        "test-build".to_owned(),
    ];
    args.extend(extra.iter().map(|value| (*value).to_owned()));
    args
}

fn ready_fixture() -> UsageBrokerForegroundReady {
    UsageBrokerForegroundReady {
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "canonical-local-source-id".to_owned(),
            surface_id: "claude".to_owned(),
        },
        binding_scope: "claude_keychain_service".to_owned(),
    }
}

#[test]
fn foreground_bootstrap_requires_all_terminal_streams_before_broker_call() {
    let args = prepare_auth_args(&[]);
    let mut called = false;
    let result = prepare_auth_with(
        &args,
        || false,
        |_, _| {
            called = true;
            panic!("headless bootstrap must not claim a lease or access Keychain")
        },
        |_| {},
    );

    assert!(!called);
    let (exit_code, json) = result.unwrap_err();
    assert_eq!(exit_code, 2);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["error"]["code"], "interaction_required");
    assert!(value["error"].get("diagnostic").is_none());
}

#[test]
fn foreground_bootstrap_passes_exact_service_and_reports_safe_source_scope() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let args = prepare_auth_args(&["--keychain-service", " Claude custom service "]);
    let ready_output = Rc::new(RefCell::new(None));
    let captured_output = Rc::clone(&ready_output);
    let result = prepare_auth_with(
        &args,
        || true,
        |request, on_ready| {
            assert_eq!(request.keychain_service, " Claude custom service ");
            assert_eq!(request.data_dir, PathBuf::from("/tmp/usage-data"));
            assert_eq!(request.config_root, PathBuf::from("/tmp/jackin-config"));
            assert_eq!(request.operator_home, PathBuf::from("/tmp/operator-home"));
            assert_eq!(request.build_id, "test-build");
            on_ready(ready_fixture());
            Ok(ForegroundBootstrapOutcome::Acquired)
        },
        move |ready| {
            *captured_output.borrow_mut() = Some(service_ready_json(&ready));
        },
    );

    result.unwrap();
    let output = ready_output.borrow().clone().unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["result"], "service_ready");
    assert_eq!(value["provider"], "claude");
    assert_eq!(value["source"]["account_id"], "canonical-local-source-id");
    assert_eq!(value["source"]["scope"], "claude_keychain_service");
    assert!(!output.contains("Claude custom service"));
}

#[test]
fn foreground_bootstrap_maps_auth_failures_without_starting_service() {
    let args = prepare_auth_args(&[]);
    let malformed = jackin_usage::usage::diagnose_claude_profile_payload(
        br#"{"claudeAiOauth":{"accessToken":"fixture-token"}}"#,
    );
    for (outcome, expected) in [
        (ForegroundBootstrapOutcome::Missing, "auth_missing"),
        (ForegroundBootstrapOutcome::Denied, "auth_denied"),
        (
            ForegroundBootstrapOutcome::InteractionRequired,
            "interaction_required",
        ),
        (
            ForegroundBootstrapOutcome::Malformed(malformed.clone()),
            "auth_malformed",
        ),
    ] {
        let result = prepare_auth_with(
            &args,
            || true,
            |_, _| Ok(outcome),
            |_| panic!("failed bootstrap must not report service readiness"),
        );
        let (exit_code, json) = result.unwrap_err();
        assert_eq!(exit_code, 2);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["error"]["code"], expected);
        assert_eq!(
            value["error"].get("diagnostic").is_some(),
            expected == "auth_malformed"
        );
    }
}

#[test]
fn malformed_auth_json_contains_only_bounded_diagnostic_facts() {
    let fixture = br#"{"claudeAiOauth":{"accessToken":"fixture-secret-token","subscriptionType":7},"oauthAccount":{"emailAddress":"fixture-private@example.test"}}"#;
    let diagnostic = jackin_usage::usage::diagnose_claude_profile_payload(fixture);
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
}

#[test]
fn foreground_lease_conflict_returns_without_authentication() {
    let args = prepare_auth_args(&[]);
    let mut ready = false;
    let result = prepare_auth_with(
        &args,
        || true,
        |_, _| {
            Err(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::BrokerConflict,
                message: "a usage broker already owns this data directory".to_owned(),
            })
        },
        |_| ready = true,
    );
    let (exit_code, json) = result.unwrap_err();
    assert!(!ready);
    assert_eq!(exit_code, 3);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["error"]["code"], "broker_conflict");
}

#[test]
fn foreground_arguments_are_validated_before_terminal_inspection() {
    let mut args = prepare_auth_args(&[]);
    args.extend(["--keychain-service".to_owned()]);
    let result = prepare_auth_with(
        &args,
        || panic!("invalid arguments must be rejected before inspecting TTYs"),
        |_, _| panic!("invalid arguments must not launch bootstrap"),
        |_| {},
    );

    let (exit_code, json) = result.unwrap_err();
    assert_eq!(exit_code, 3);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["error"]["code"], "invalid_request");
}

#[test]
fn prepare_auth_mode_is_detected_before_detach() {
    assert!(prepare_auth_requested(&prepare_auth_args(&[])));
    assert!(!prepare_auth_requested(&[
        "jackin-usage-broker".to_owned(),
        "--data-dir".to_owned(),
        "/tmp/usage-data".to_owned(),
    ]));
    assert!(!prepare_auth_requested(&[
        "jackin-usage-broker".to_owned(),
        "--keychain-service".to_owned(),
        "--prepare-auth".to_owned(),
    ]));
}
