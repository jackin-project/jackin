// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::monitor::{
    AccountObservations, DurableGoalSpend, SpendAccountState, SpendState, StoreState,
};
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorPolicy, MonitorPolicyOrigin, MonitorPolicyRecord,
    MonitorProvider, SpendRecord, SpendRecordSource, SpendVerification,
};
use std::os::unix::fs::PermissionsExt as _;

fn legacy_snapshot(version: u16) -> Vec<u8> {
    let mut state = StoreState {
        schema_version: version,
        last_now_epoch: 10,
        next_binding_id: 2,
        ..StoreState::default()
    };
    state
        .accounts
        .insert("account-1".to_owned(), AccountObservations::default());
    state.bindings.insert(
        "binding-00000001".to_owned(),
        vec![MonitorAccountBinding {
            binding_id: "binding-00000001".to_owned(),
            provider: MonitorProvider::Claude,
            account_id: "account-1".to_owned(),
            provider_account_id: None,
            experimental_collector_approved: false,
            operator_label: "migrated".to_owned(),
            revision: 1,
            operator_confirmed: true,
            confirmed_at_epoch: Some(10),
        }],
    );

    let mut value = serde_json::to_value(state).expect("serialize current-shaped fixture");
    let binding = &mut value["bindings"]["binding-00000001"][0];
    binding
        .as_object_mut()
        .expect("binding object")
        .remove("provider_account_id");
    binding
        .as_object_mut()
        .expect("binding object")
        .remove("experimental_collector_approved");
    value["accounts"]["account-1"]["spend"]
        .as_object_mut()
        .expect("spend account object")
        .remove("historical_correction_horizon_epoch");
    serde_json::to_vec(&value).expect("serialize old snapshot")
}

fn write_snapshot(data_dir: &Path, bytes: &[u8]) -> (File, std::path::PathBuf) {
    let directory = open_store_dir(data_dir).expect("open private store directory");
    let path = data_dir
        .join(crate::BROKER_DIR)
        .join(STORE_DIR)
        .join(STORE_FILE);
    std::fs::write(&path, bytes).expect("write snapshot fixture");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(STORE_MODE))
        .expect("set owner-only snapshot permissions");
    (directory, path)
}

#[test]
fn v2_and_v3_default_new_source_and_horizon_fields_off() {
    for version in [2, 3] {
        let state = migrate_v2_or_v3(&legacy_snapshot(version), version)
            .expect("old schema should migrate safely");
        assert_eq!(state.schema_version, USAGE_MONITOR_SCHEMA_VERSION);
        let binding = &state.bindings["binding-00000001"][0];
        assert_eq!(binding.provider_account_id, None);
        assert!(!binding.experimental_collector_approved);
        assert_eq!(
            state.accounts["account-1"]
                .spend
                .historical_correction_horizon_epoch,
            None
        );
    }
}

#[test]
fn v3_latches_only_rolled_strict_spend_and_preserves_estimate() {
    let baseline = SpendRecord {
        account_id: "account-1".to_owned(),
        billing_period_start_epoch: 100,
        billing_period_end_epoch: 200,
        amount: Money::new(7_000, "SGD", 2),
        evidence_at_epoch: Some(150),
        evidence_received_at_epoch: 150,
        source: SpendRecordSource::OperatorReceipt,
        verification: SpendVerification::Verified,
    };
    let current = SpendRecord {
        account_id: "account-1".to_owned(),
        billing_period_start_epoch: 200,
        billing_period_end_epoch: 300,
        amount: Money::new(200, "SGD", 2),
        evidence_at_epoch: Some(240),
        evidence_received_at_epoch: 240,
        source: SpendRecordSource::OperatorReceipt,
        verification: SpendVerification::Verified,
    };
    let mut state = StoreState {
        last_now_epoch: 250,
        ..StoreState::default()
    };
    state.accounts.insert(
        "account-1".to_owned(),
        AccountObservations {
            spend: SpendAccountState {
                latest_record: Some(current.clone()),
                current_period_record: Some(current.clone()),
                previous_period_record: None,
                historical_correction_horizon_epoch: None,
            },
            ..AccountObservations::default()
        },
    );
    state.goals.insert(
        "goal-1".to_owned(),
        DurableGoalSpend {
            account_id: "account-1".to_owned(),
            binding_id: "binding-00000001".to_owned(),
            binding_revision: 1,
            policy_revision: 1,
            policy: MonitorPolicy::StrictSgd,
            budget: Some(Money::new(10_000, "SGD", 2)),
            spend_state: Some(SpendState {
                baseline: Some(baseline),
                period_anchor: Some(current.clone()),
                closed_period_anchor: None,
                cumulative_goal_spend: Some(Money::new(7_200, "SGD", 2)),
                rollover_unknown: false,
                cumulative_complete: true,
            }),
        },
    );

    let changed = latch_ambiguous_v3_rollovers(&mut state);
    assert_eq!(changed, BTreeSet::from(["goal-1".to_owned()]));
    let spend = state.goals["goal-1"]
        .spend_state
        .as_ref()
        .expect("goal spend state");
    assert_eq!(
        spend.cumulative_goal_spend,
        Some(Money::new(7_200, "SGD", 2))
    );
    assert!(spend.rollover_unknown);
    assert!(!spend.cumulative_complete);
    assert_eq!(
        state.accounts["account-1"]
            .spend
            .historical_correction_horizon_epoch,
        Some(200)
    );
}

#[test]
fn load_atomically_rewrites_v3_and_fails_closed_without_overwriting_invalid_state() {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let data_dir = temp.path().join("data");
    let (directory, path) = write_snapshot(&data_dir, &legacy_snapshot(3));

    let migrated = load(&directory)
        .expect("load migrated v3 state")
        .expect("snapshot exists");
    assert_eq!(migrated.schema_version, USAGE_MONITOR_SCHEMA_VERSION);
    let rewritten = std::fs::read(&path).expect("read published v4 state");
    let value: serde_json::Value =
        serde_json::from_slice(&rewritten).expect("valid published JSON");
    assert_eq!(value["schema_version"], USAGE_MONITOR_SCHEMA_VERSION);
    assert_eq!(
        std::fs::metadata(&path).expect("state metadata").mode() & 0o777,
        STORE_MODE
    );
    assert_eq!(
        std::fs::read_dir(path.parent().expect("store directory"))
            .expect("read store directory")
            .count(),
        1,
        "atomic rename leaves no staging file"
    );

    let bad_temp = tempfile::tempdir().expect("invalid fixture root");
    let bad_data_dir = bad_temp.path().join("data");
    let mut malformed: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot(3)).expect("parse legacy fixture");
    malformed["accounts"]["account-1"]["spend"]["historical_correction_horizon_epoch"] =
        serde_json::json!(-1);
    let malformed = serde_json::to_vec(&malformed).expect("serialize malformed fixture");
    let (bad_directory, bad_path) = write_snapshot(&bad_data_dir, &malformed);
    load(&bad_directory).unwrap_err();
    assert_eq!(
        std::fs::read(&bad_path).expect("invalid source remains inspectable"),
        malformed,
        "invalid source is not replaced by a migrated snapshot"
    );

    let future_temp = tempfile::tempdir().expect("future fixture root");
    let future_data_dir = future_temp.path().join("data");
    let mut future: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot(3)).expect("parse legacy fixture");
    future["schema_version"] = serde_json::json!(5);
    let future = serde_json::to_vec(&future).expect("serialize future fixture");
    let (future_directory, future_path) = write_snapshot(&future_data_dir, &future);
    load(&future_directory).unwrap_err();
    assert_eq!(
        std::fs::read(&future_path).expect("future source remains untouched"),
        future
    );

    let schema6_temp = tempfile::tempdir().expect("schema 6 fixture root");
    let schema6_data_dir = schema6_temp.path().join("data");
    let mut schema6: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot(3)).expect("parse legacy fixture");
    schema6["schema_version"] = serde_json::json!(6);
    let schema6 = serde_json::to_vec(&schema6).expect("serialize schema 6 fixture");
    let (schema6_directory, schema6_path) = write_snapshot(&schema6_data_dir, &schema6);
    load(&schema6_directory).unwrap_err();
    assert_eq!(
        std::fs::read(&schema6_path).expect("schema 6 source remains untouched"),
        schema6
    );
}

#[test]
fn load_rejects_and_preserves_v3_policy_history_that_loosens_a_strict_budget() {
    let mut state: StoreState =
        serde_json::from_slice(&legacy_snapshot(3)).expect("parse v3 snapshot fixture");
    state.policy_records.insert(
        "goal-history-test".to_owned(),
        vec![
            MonitorPolicyRecord {
                provider: MonitorProvider::Claude,
                account_id: "account-1".to_owned(),
                binding_id: Some("binding-00000001".to_owned()),
                binding_revision: Some(1),
                goal_id: "goal-history-test".to_owned(),
                previous_policy: None,
                new_policy: MonitorPolicy::StrictSgd,
                budget: Some(Money::new(5_000, "SGD", 2)),
                operator_label: Some("operator".to_owned()),
                operator_confirmed: true,
                acknowledge_no_sgd_cap: false,
                recorded_at_epoch: Some(2),
                revision: 1,
                origin: MonitorPolicyOrigin::Operator,
            },
            MonitorPolicyRecord {
                provider: MonitorProvider::Claude,
                account_id: "account-1".to_owned(),
                binding_id: Some("binding-00000001".to_owned()),
                binding_revision: Some(1),
                goal_id: "goal-history-test".to_owned(),
                previous_policy: Some(MonitorPolicy::StrictSgd),
                new_policy: MonitorPolicy::StrictSgd,
                budget: Some(Money::new(5_001, "SGD", 2)),
                operator_label: Some("operator".to_owned()),
                operator_confirmed: true,
                acknowledge_no_sgd_cap: false,
                recorded_at_epoch: Some(3),
                revision: 2,
                origin: MonitorPolicyOrigin::Operator,
            },
        ],
    );
    let bytes = serde_json::to_vec(&state).expect("serialize invalid old-schema fixture");
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let data_dir = temp.path().join("data");
    let (directory, path) = write_snapshot(&data_dir, &bytes);

    assert!(
        load(&directory).is_err(),
        "invalid history must fail closed"
    );
    assert_eq!(
        std::fs::read(&path).expect("invalid v3 source remains inspectable"),
        bytes,
        "invalid policy history must not be restamped as schema 4"
    );
}
