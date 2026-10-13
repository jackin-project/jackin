// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disc_env_per_key_never_resolves_unrelated_or_on_demand_values() {
    let mut config = AppConfig::default();
    config.env.insert(
        "CONTEXT7_API_KEY".to_owned(),
        op_ref("context7", None, false),
    );
    config
        .env
        .insert("SERVICE_A_TOKEN".to_owned(), op_ref("zai", None, true));
    let runner = FakeOpRunner::default();

    let results =
        resolve_operator_env_per_key_with_matching(&config, None, None, &runner, host_env, |key| {
            key == "SERVICE_A_TOKEN"
        });

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key(), "SERVICE_A_TOKEN");
    assert_eq!(
        results[0].status(),
        OperatorEnvKeyStatus::InteractionRequired
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn account_declaration_resolution_preserves_status_and_redacts_secrets() {
    let runner = FakeOpRunner::default();
    for (declaration, expected) in [
        (
            EnvValue::from("fixture-account-secret"),
            OperatorEnvKeyStatus::Resolved,
        ),
        (EnvValue::from("   "), OperatorEnvKeyStatus::Malformed),
        (
            EnvValue::from("$ABSENT_ACCOUNT_SECRET"),
            OperatorEnvKeyStatus::Missing,
        ),
        (
            EnvValue::OpRef(OpRef {
                op: "op://vault/account/key".into(),
                path: "Account/key".into(),
                account: None,
                on_demand: false,
            }),
            OperatorEnvKeyStatus::DeniedOrUnavailable,
        ),
        (
            EnvValue::OpRef(OpRef {
                op: "op://vault/account/key".into(),
                path: "Account/key".into(),
                account: None,
                on_demand: true,
            }),
            OperatorEnvKeyStatus::InteractionRequired,
        ),
    ] {
        let resolution =
            resolve_account_declaration_with("OPENAI_API_KEY", &declaration, &runner, |_| {
                Err(std::env::VarError::NotPresent)
            });
        assert_eq!(resolution.status(), expected);
        assert!(!format!("{resolution:?}").contains("fixture-account-secret"));
        if expected == OperatorEnvKeyStatus::Resolved {
            assert_eq!(resolution.resolved_value(), Some("fixture-account-secret"));
        } else {
            assert!(resolution.resolved_value().is_none());
        }
    }
}
