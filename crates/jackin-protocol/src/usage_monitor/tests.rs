// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    MonitorEvidenceFreshness, MonitorFieldEvidence, MonitorOperation, MonitorQuotaWindowStatus,
    SpendRecordInput, SpendRecordSource, StatuslineObservation, USAGE_MONITOR_MAX_STATUSLINE_BYTES,
};
use crate::control::Money;

#[test]
fn service_status_exposes_secret_free_foreground_collector_mode() {
    let foreground = super::MonitorServiceStatus {
        running: true,
        experimental_collector_source: Some("claude-source-account".to_owned()),
        active_monitors: 1,
        next_wake_epoch: Some(1_800_000_000),
    };
    let value = serde_json::to_value(&foreground).expect("service status should serialize");
    assert_eq!(
        value["experimental_collector_source"],
        "claude-source-account"
    );

    let passive = super::MonitorServiceStatus {
        running: true,
        experimental_collector_source: None,
        active_monitors: 0,
        next_wake_epoch: None,
    };
    let value = serde_json::to_value(&passive).expect("passive service status should serialize");
    assert!(value["experimental_collector_source"].is_null());
}

#[test]
fn collector_auth_required_has_stable_wire_code() {
    let issue = super::MonitorIssue {
        code: super::MonitorIssueCode::CollectorAuthRequired,
        message: "experimental collection requires foreground setup".to_owned(),
        retry_at_epoch: None,
    };
    let value = serde_json::to_value(issue).expect("collector issue should serialize");
    assert_eq!(value["code"], "collector_auth_required");
}

#[test]
fn binding_input_defaults_experimental_collector_approval_to_false() {
    let input = serde_json::json!({
        "provider": "claude",
        "account_id": "local-account",
        "provider_account_id": "source-account",
        "operator_label": "work",
        "operator_confirmed": true
    });
    let binding: super::MonitorAccountBindingInput =
        serde_json::from_value(input).expect("binding input without approval should decode");

    assert!(!binding.experimental_collector_approved);
}

#[test]
fn statusline_contract_preserves_window_values_in_basis_points() {
    let input = r#"{
            "schema_version": 2,
            "session_id": "session-1",
            "model": "claude-sonnet",
            "claude_code_version": "2.1.80",
            "rate_limits": {
                "five_hour": {"used_percentage_basis_points": 1234, "reset_at_epoch": 1800000000},
                "seven_day": {"used_percentage_basis_points": 9000, "reset_at_epoch": 1800600000}
            }
        }"#;
    let observation: StatuslineObservation =
        serde_json::from_str(input).expect("normalized statusline input should decode");

    assert_eq!(
        observation
            .rate_limits
            .five_hour
            .as_ref()
            .and_then(|w| w.used_percentage_basis_points),
        Some(1234)
    );
    assert_eq!(
        observation
            .rate_limits
            .seven_day
            .as_ref()
            .and_then(|w| w.used_percentage_basis_points),
        Some(9000)
    );
    assert_eq!(observation.claude_code_version.as_deref(), Some("2.1.80"));
    assert_eq!(USAGE_MONITOR_MAX_STATUSLINE_BYTES, 16 * 1024);
}

#[test]
fn statusline_ingest_is_a_typed_monitor_operation() {
    let operation = MonitorOperation::Ingest {
        scope: super::MonitorScope::Session {
            session_id: "session-1".to_owned(),
        },
        observation: StatuslineObservation {
            schema_version: super::USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
            session_id: "session-1".to_owned(),
            model: Some("claude-sonnet".to_owned()),
            claude_code_version: Some("2.1.80".to_owned()),
            ..StatuslineObservation::default()
        },
    };
    let value = serde_json::to_value(operation).expect("monitor operation should encode");

    assert_eq!(value["operation"], "ingest");
    assert_eq!(value["scope"]["scope"], "session");
    assert_eq!(value["scope"]["session_id"], "session-1");
    assert_eq!(value["observation"]["claude_code_version"], "2.1.80");
    assert!(value["observation"].get("evidence_at_epoch").is_none());
}

#[test]
fn v4_monitor_control_shapes_are_tagged_and_secret_free() {
    use super::{
        MonitorAccountBindingInput, MonitorConfig, MonitorOperation, MonitorPolicy,
        MonitorPolicyApprovalInput, MonitorPurpose, MonitorScope, USAGE_MONITOR_SCHEMA_VERSION,
    };

    let binding = MonitorOperation::BindAccount {
        binding: MonitorAccountBindingInput {
            provider: super::MonitorProvider::Claude,
            account_id: "local-account".to_owned(),
            provider_account_id: Some("claude-account-1".to_owned()),
            experimental_collector_approved: true,
            operator_label: "work account".to_owned(),
            operator_confirmed: true,
        },
    };
    let binding_value = serde_json::to_value(binding).expect("binding should encode");
    assert_eq!(binding_value["operation"], "bind_account");
    assert_eq!(binding_value["binding"]["operator_confirmed"], true);
    assert_eq!(
        binding_value["binding"]["experimental_collector_approved"],
        true
    );
    assert_eq!(
        binding_value["binding"]["provider_account_id"],
        "claude-account-1"
    );
    assert!(binding_value["binding"].get("credential").is_none());

    let approval = MonitorOperation::ApprovePolicy {
        approval: MonitorPolicyApprovalInput {
            binding_id: "binding-1".to_owned(),
            binding_revision: 1,
            goal_id: "goal-1".to_owned(),
            new_policy: MonitorPolicy::QuotaOnly,
            budget: None,
            operator_label: "operator".to_owned(),
            operator_confirmed: true,
            acknowledge_no_sgd_cap: true,
            expected_revision: None,
        },
    };
    let approval_value = serde_json::to_value(approval).expect("approval should encode");
    assert_eq!(approval_value["operation"], "approve_policy");
    assert_eq!(approval_value["approval"]["new_policy"], "quota_only");
    assert_eq!(approval_value["approval"]["acknowledge_no_sgd_cap"], true);

    let start = MonitorOperation::Start {
        config: MonitorConfig {
            provider: super::MonitorProvider::Claude,
            purpose: MonitorPurpose::ObserveOnly,
            scope: MonitorScope::BoundAccount {
                binding_id: "binding-1".to_owned(),
                binding_revision: 1,
                session_id: Some("session-1".to_owned()),
            },
            goal_id: None,
            expected_model: None,
            policy_revision: None,
            experimental_collector: false,
        },
        idempotency_key: "retry-1".to_owned(),
    };
    let start_value = serde_json::to_value(&start).expect("start should encode");
    let decoded: MonitorOperation =
        serde_json::from_value(start_value.clone()).expect("v4 start should decode");
    assert_eq!(decoded, start);
    assert_eq!(start_value["config"]["purpose"], "observe_only");
    assert_eq!(start_value["idempotency_key"], "retry-1");
    let mut legacy_start = start_value.clone();
    legacy_start["config"]["budget"] = serde_json::json!({
        "amount_minor": 5_000,
        "currency": "SGD",
        "exponent": 2
    });
    serde_json::from_value::<MonitorOperation>(legacy_start)
        .expect_err("legacy budget override must be rejected");
    assert_eq!(USAGE_MONITOR_SCHEMA_VERSION, 5);
    assert_eq!(crate::usage_broker::USAGE_BROKER_PROTOCOL_VERSION, "v9");
    let mut pre_opt_in_start = start_value;
    pre_opt_in_start["config"]
        .as_object_mut()
        .expect("monitor config should encode as an object")
        .remove("experimental_collector");
    let decoded: MonitorOperation = serde_json::from_value(pre_opt_in_start)
        .expect("a missing experimental collector field defaults to disabled");
    assert!(matches!(
        decoded,
        MonitorOperation::Start { config, .. } if !config.experimental_collector
    ));
    serde_json::from_value::<MonitorOperation>(serde_json::json!({
        "operation": "prepare_auth",
        "provider": "claude"
    }))
    .expect_err("authentication bootstrap is not a monitor wire operation");
    serde_json::from_value::<super::MonitorReply>(serde_json::json!({
        "result": "auth_prepared",
        "provider": "claude",
        "issues": []
    }))
    .expect_err("authentication bootstrap has no wire reply");
    assert_eq!(
        serde_json::to_value(super::MonitorBudgetReadiness::Disabled)
            .expect("readiness should encode"),
        "disabled"
    );
    assert_eq!(
        serde_json::to_value(super::MonitorDispatchReadiness::NotAuthorized)
            .expect("readiness should encode"),
        "not_authorized"
    );
}

#[test]
fn missing_statusline_version_stays_unknown() {
    let input = r#"{
            "schema_version": 2,
            "session_id": "session-1",
            "model": null,
            "rate_limits": {"five_hour": null, "seven_day": null}
        }"#;
    let observation: StatuslineObservation =
        serde_json::from_str(input).expect("missing optional version should decode as unknown");

    assert_eq!(observation.claude_code_version, None);
    let encoded = serde_json::to_value(observation).expect("observation should encode");
    assert!(encoded["claude_code_version"].is_null());
    assert!(encoded.get("evidence_at_epoch").is_none());
}

#[test]
fn model_guard_validity_states_have_stable_names() {
    use super::MonitorModelGuardValidity;

    assert_eq!(
        serde_json::to_value(MonitorModelGuardValidity::NotConfigured)
            .expect("model guard state should encode"),
        "not_configured"
    );
    assert_eq!(
        serde_json::to_value(MonitorModelGuardValidity::Unknown)
            .expect("model guard state should encode"),
        "unknown"
    );
    assert_eq!(
        serde_json::to_value(MonitorModelGuardValidity::Match)
            .expect("model guard state should encode"),
        "match"
    );
    assert_eq!(
        serde_json::to_value(MonitorModelGuardValidity::Mismatch)
            .expect("model guard state should encode"),
        "mismatch"
    );
}

#[test]
fn migrated_policy_timestamp_remains_unknown() {
    use super::{MonitorPolicy, MonitorPolicyOrigin, MonitorPolicyRecord, MonitorProvider};

    let migrated = MonitorPolicyRecord {
        provider: MonitorProvider::Claude,
        account_id: "account-1".to_owned(),
        binding_id: None,
        binding_revision: None,
        goal_id: "goal-1".to_owned(),
        previous_policy: None,
        new_policy: MonitorPolicy::StrictSgd,
        budget: None,
        operator_label: None,
        operator_confirmed: false,
        acknowledge_no_sgd_cap: false,
        recorded_at_epoch: None,
        revision: 1,
        origin: MonitorPolicyOrigin::MigratedV1,
    };
    let encoded = serde_json::to_value(&migrated).expect("policy should encode");
    assert!(encoded["recorded_at_epoch"].is_null());

    let mut unknown_timestamp = encoded;
    unknown_timestamp
        .as_object_mut()
        .expect("policy should encode as an object")
        .remove("recorded_at_epoch");
    let decoded: MonitorPolicyRecord = serde_json::from_value(unknown_timestamp)
        .expect("an absent migrated policy timestamp should remain unknown");
    assert_eq!(decoded.recorded_at_epoch, None);
}

#[test]
fn migrated_binding_confirmation_time_remains_unknown() {
    use super::{MonitorAccountBinding, MonitorProvider};

    let migrated = MonitorAccountBinding {
        binding_id: "legacy-binding-1".to_owned(),
        provider: MonitorProvider::Claude,
        account_id: "account-1".to_owned(),
        provider_account_id: Some("source-account".to_owned()),
        experimental_collector_approved: false,
        operator_label: "migrated v1 binding".to_owned(),
        revision: 1,
        operator_confirmed: false,
        confirmed_at_epoch: None,
    };
    let encoded = serde_json::to_value(&migrated).expect("binding should encode");
    assert!(encoded["confirmed_at_epoch"].is_null());

    let mut unknown_timestamp = encoded;
    let unknown_binding_fields = unknown_timestamp
        .as_object_mut()
        .expect("binding should encode as an object");
    unknown_binding_fields.remove("confirmed_at_epoch");
    unknown_binding_fields.remove("experimental_collector_approved");
    let decoded: MonitorAccountBinding = serde_json::from_value(unknown_timestamp)
        .expect("an absent unconfirmed binding timestamp should remain unknown");
    assert_eq!(decoded.confirmed_at_epoch, None);
    assert_eq!(
        decoded.provider_account_id.as_deref(),
        Some("source-account")
    );
    assert!(!decoded.experimental_collector_approved);
    assert!(!decoded.operator_confirmed);

    let confirmed = MonitorAccountBinding {
        operator_confirmed: true,
        provider_account_id: Some("provider-account-1".to_owned()),
        confirmed_at_epoch: Some(1_800_000_000),
        ..decoded
    };
    let confirmed_value = serde_json::to_value(confirmed).expect("binding should encode");
    assert_eq!(confirmed_value["confirmed_at_epoch"], 1_800_000_000);
    assert_eq!(confirmed_value["provider_account_id"], "provider-account-1");
}

#[test]
fn quota_window_status_keeps_used_and_reset_evidence_independent() {
    let status = MonitorQuotaWindowStatus {
        used_percentage_basis_points: Some(9_000),
        used_evidence: Some(MonitorFieldEvidence {
            evidence_sequence: 12,
            evidence_at_epoch: None,
            evidence_received_at_epoch: 1_800_000_000,
            age_seconds: 5,
            freshness: MonitorEvidenceFreshness::Current,
        }),
        reset_at_epoch: Some(1_800_060_000),
        reset_validity: super::MonitorResetValidity::Future,
        reset_evidence: Some(MonitorFieldEvidence {
            evidence_sequence: 8,
            evidence_at_epoch: Some(1_799_999_000),
            evidence_received_at_epoch: 1_800_000_000,
            age_seconds: 1_000,
            freshness: MonitorEvidenceFreshness::Stale,
        }),
    };
    let value = serde_json::to_value(status).expect("window status should encode");

    assert_eq!(value["used_percentage_basis_points"], 9_000);
    assert_eq!(value["used_evidence"]["evidence_sequence"], 12);
    assert_eq!(value["reset_validity"], "future");
    assert_eq!(value["reset_evidence"]["evidence_sequence"], 8);
    assert_eq!(value["reset_evidence"]["freshness"], "stale");
}

#[test]
fn unbound_session_evidence_does_not_claim_an_account() {
    let evidence = super::MonitorEvidence {
        sequence: 1,
        account_id: None,
        session_id: Some("session-1".to_owned()),
        claude_code_version: Some("2.1.80".to_owned()),
        source: super::MonitorEvidenceSource::Statusline,
        evidence_at_epoch: None,
        evidence_received_at_epoch: 1_800_000_000,
        age_seconds: 0,
        value: super::MonitorEvidenceValue::Model {
            model: "claude-sonnet".to_owned(),
        },
    };
    let value = serde_json::to_value(evidence).expect("evidence should encode");

    assert!(value["account_id"].is_null());
    assert_eq!(value["session_id"], "session-1");
    assert_eq!(value["claude_code_version"], "2.1.80");
}

#[test]
fn spend_input_requires_account_period_currency_and_source() {
    let input = SpendRecordInput {
        account_id: "account-1".to_owned(),
        billing_period_start_epoch: 1_800_000_000,
        billing_period_end_epoch: 1_802_592_000,
        amount: Money::new(5_000, "SGD", 2),
        evidence_at_epoch: Some(1_800_000_000),
        verified: true,
        source: SpendRecordSource::OperatorReceipt,
    };
    let value = serde_json::to_value(input).expect("spend input should encode");

    assert_eq!(value["account_id"], "account-1");
    assert_eq!(value["amount"]["currency"], "SGD");
    assert_eq!(value["verified"], true);
    assert_eq!(value["source"], "operator_receipt");
    assert!(value.get("evidence_received_at_epoch").is_none());
}
