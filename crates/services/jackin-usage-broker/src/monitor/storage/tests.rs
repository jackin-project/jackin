// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::monitor::{
    AccountObservations, DurableGoalSpend, Observed, ObservedPercentage, ObservedQuotaPair,
    ObservedWindow, ResetAdmissionFence, SpendAccountState, SpendState, StoreState,
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
    if version <= 3 {
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
    }
    value["accounts"]["account-1"]
        .as_object_mut()
        .expect("account observations")
        .remove("reset_admission_fences");
    value["accounts"]["account-1"]
        .as_object_mut()
        .expect("account observations")
        .remove("used_only_admission_sequences");
    serde_json::to_vec(&value).expect("serialize old snapshot")
}

struct LegacyUsedWindow {
    value: i32,
    reset_at_epoch: i64,
    source: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
}

struct LegacyResetWindow {
    value: i64,
    source: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
}

struct LegacyPairWindow {
    used_percentage_basis_points: i32,
    reset_at_epoch: i64,
    source_summary: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
}

fn legacy_pair_window(
    used: LegacyUsedWindow,
    reset: LegacyResetWindow,
    paired: LegacyPairWindow,
) -> serde_json::Value {
    serde_json::json!({
        "used": {
            "value": used.value,
            "reset_at_epoch": used.reset_at_epoch,
            "evidence_at_epoch": used.source,
            "received_at_epoch": used.received_at_epoch,
            "input_sequence": used.input_sequence,
            "claude_code_version": null
        },
        "reset": {
            "value": reset.value,
            "evidence_at_epoch": reset.source,
            "received_at_epoch": reset.received_at_epoch,
            "input_sequence": reset.input_sequence,
            "claude_code_version": null
        },
        "paired": {
            "used_percentage_basis_points": paired.used_percentage_basis_points,
            "reset_at_epoch": paired.reset_at_epoch,
            "evidence_at_epoch": paired.source_summary,
            "received_at_epoch": paired.received_at_epoch,
            "input_sequence": paired.input_sequence,
            "claude_code_version": null
        }
    })
}

fn empty_legacy_window() -> serde_json::Value {
    serde_json::json!({"used": null, "reset": null, "paired": null})
}

fn populate_legacy_pair_migration_account(account: &mut serde_json::Value) {
    account["input_sequence"] = serde_json::json!(3);
    account["latest_reset_epochs"] = serde_json::json!([200, 100]);
    // The broker reset-only update keeps used and its old pair at sequence1.
    account["broker_windows"][0] = legacy_pair_window(
        LegacyUsedWindow {
            value: 5_400,
            reset_at_epoch: 100,
            source: Some(10),
            received_at_epoch: 10,
            input_sequence: 1,
        },
        LegacyResetWindow {
            value: 200,
            source: Some(20),
            received_at_epoch: 20,
            input_sequence: 2,
        },
        LegacyPairWindow {
            used_percentage_basis_points: 5_400,
            reset_at_epoch: 100,
            source_summary: Some(10),
            received_at_epoch: 10,
            input_sequence: 1,
        },
    );
    // One projection operation selects max-used and min-reset from distinct sources.
    account["broker_windows"][1] = legacy_pair_window(
        LegacyUsedWindow {
            value: 7_200,
            reset_at_epoch: 100,
            source: Some(10),
            received_at_epoch: 20,
            input_sequence: 3,
        },
        LegacyResetWindow {
            value: 100,
            source: Some(11),
            received_at_epoch: 20,
            input_sequence: 3,
        },
        LegacyPairWindow {
            used_percentage_basis_points: 7_200,
            reset_at_epoch: 100,
            source_summary: Some(10),
            received_at_epoch: 20,
            input_sequence: 3,
        },
    );
    account["sessions"]["bound-session"] = serde_json::json!({
        "model": null,
        "windows": [
            // A statusline reset-only update retains the complete54 pair from sequence1.
            legacy_pair_window(
                LegacyUsedWindow {
                    value: 5_400,
                    reset_at_epoch: 100,
                    source: None,
                    received_at_epoch: 10,
                    input_sequence: 1,
                },
                LegacyResetWindow {
                    value: 200,
                    source: None,
                    received_at_epoch: 20,
                    input_sequence: 2,
                },
                LegacyPairWindow {
                    used_percentage_basis_points: 5_400,
                    reset_at_epoch: 100,
                    source_summary: None,
                    received_at_epoch: 10,
                    input_sequence: 1,
                },
            ),
            // complete54/resetR followed by complete60/same-reset retains old reset receipt.
            legacy_pair_window(
                LegacyUsedWindow {
                    value: 6_000,
                    reset_at_epoch: 100,
                    source: None,
                    received_at_epoch: 20,
                    input_sequence: 2,
                },
                LegacyResetWindow {
                    value: 100,
                    source: None,
                    received_at_epoch: 10,
                    input_sequence: 1,
                },
                LegacyPairWindow {
                    used_percentage_basis_points: 6_000,
                    reset_at_epoch: 100,
                    source_summary: None,
                    received_at_epoch: 20,
                    input_sequence: 2,
                },
            )
        ],
        "last_observation_received_at_epoch": 20,
        "claude_code_version": null,
        "last_callback_received_at_epoch": 20
    });
    account["sessions"]["source-shape-mismatch"] = serde_json::json!({
        "model": null,
        "windows": [
            legacy_pair_window(
                LegacyUsedWindow {
                    value: 7_200,
                    reset_at_epoch: 100,
                    source: Some(10),
                    received_at_epoch: 20,
                    input_sequence: 3,
                },
                LegacyResetWindow {
                    value: 100,
                    source: Some(11),
                    received_at_epoch: 20,
                    input_sequence: 3,
                },
                LegacyPairWindow {
                    used_percentage_basis_points: 7_200,
                    reset_at_epoch: 100,
                    source_summary: Some(10),
                    received_at_epoch: 20,
                    input_sequence: 3,
                },
            ),
            empty_legacy_window()
        ],
        "last_observation_received_at_epoch": 20,
        "claude_code_version": null,
        "last_callback_received_at_epoch": 20
    });
}

fn legacy_pair_migration_snapshot(version: u16) -> Vec<u8> {
    let mut old: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot(version)).expect("parse old snapshot");
    old["last_now_epoch"] = serde_json::json!(100);
    old["next_input_sequence"] = serde_json::json!(3);
    {
        let account = &mut old["accounts"]["account-1"];
        populate_legacy_pair_migration_account(account);
    }
    old["unbound_sessions"]["unbound-session"] = serde_json::json!({
        "model": null,
        "windows": [
            empty_legacy_window(),
            legacy_pair_window(
                LegacyUsedWindow {
                    value: 7_200,
                    reset_at_epoch: 100,
                    source: None,
                    received_at_epoch: 20,
                    input_sequence: 3,
                },
                LegacyResetWindow {
                    value: 100,
                    source: None,
                    received_at_epoch: 20,
                    input_sequence: 3,
                },
                LegacyPairWindow {
                    used_percentage_basis_points: 7_200,
                    reset_at_epoch: 100,
                    source_summary: None,
                    received_at_epoch: 20,
                    input_sequence: 3,
                },
            )
        ],
        "last_observation_received_at_epoch": 20,
        "claude_code_version": null,
        "last_callback_received_at_epoch": 20
    });
    let mut broker_wrong_source: serde_json::Value = serde_json::to_value(AccountObservations {
        input_sequence: 3,
        latest_reset_epochs: [Some(100), None],
        broker_windows: [
            ObservedWindow {
                used: Some(ObservedPercentage {
                    value: 7_200,
                    reset_at_epoch: Some(100),
                    evidence_at_epoch: None,
                    received_at_epoch: 20,
                    input_sequence: 3,
                    claude_code_version: None,
                }),
                reset: Some(Observed {
                    value: 100,
                    evidence_at_epoch: None,
                    received_at_epoch: 20,
                    input_sequence: 3,
                    claude_code_version: None,
                }),
                paired: Some(ObservedQuotaPair {
                    used_percentage_basis_points: 7_200,
                    reset_at_epoch: 100,
                    evidence_at_epoch: None,
                    source_times: [None, None],
                    received_at_epoch: 20,
                    input_sequence: 3,
                    claude_code_version: None,
                }),
            },
            ObservedWindow::default(),
        ],
        ..AccountObservations::default()
    })
    .expect("serialize receipt-only broker pair");
    let removed_source_times = broker_wrong_source["broker_windows"][0]["paired"]
        .as_object_mut()
        .expect("receipt-only legacy pair object")
        .remove("source_times");
    assert_eq!(
        removed_source_times,
        Some(serde_json::json!([null, null])),
        "legacy fixture omits only the current schema's source tuple"
    );
    if version <= 3 {
        let removed_horizon = broker_wrong_source["spend"]
            .as_object_mut()
            .expect("legacy spend object")
            .remove("historical_correction_horizon_epoch");
        assert_eq!(
            removed_horizon,
            Some(serde_json::Value::Null),
            "v2 and v3 fixtures omit only the newer null spend horizon"
        );
    }
    broker_wrong_source
        .as_object_mut()
        .expect("account object")
        .remove("reset_admission_fences");
    broker_wrong_source
        .as_object_mut()
        .expect("account object")
        .remove("used_only_admission_sequences");
    old["accounts"]["account-broker-source-shape"] = broker_wrong_source;
    serde_json::to_vec(&old).expect("serialize callback-sequence snapshot")
}

fn legacy_v1_pair_snapshot() -> Vec<u8> {
    let mut broker_window = legacy_pair_window(
        LegacyUsedWindow {
            value: 7_200,
            reset_at_epoch: 100,
            source: Some(10),
            received_at_epoch: 20,
            input_sequence: 3,
        },
        LegacyResetWindow {
            value: 100,
            source: Some(11),
            received_at_epoch: 20,
            input_sequence: 3,
        },
        LegacyPairWindow {
            used_percentage_basis_points: 7_200,
            reset_at_epoch: 100,
            source_summary: Some(10),
            received_at_epoch: 20,
            input_sequence: 3,
        },
    );
    let mut session_window = legacy_pair_window(
        LegacyUsedWindow {
            value: 6_000,
            reset_at_epoch: 100,
            source: None,
            received_at_epoch: 20,
            input_sequence: 2,
        },
        LegacyResetWindow {
            value: 100,
            source: None,
            received_at_epoch: 10,
            input_sequence: 1,
        },
        LegacyPairWindow {
            used_percentage_basis_points: 6_000,
            reset_at_epoch: 100,
            source_summary: None,
            received_at_epoch: 20,
            input_sequence: 2,
        },
    );
    let mut coherent_statusline_window = legacy_pair_window(
        LegacyUsedWindow {
            value: 7_200,
            reset_at_epoch: 100,
            source: None,
            received_at_epoch: 20,
            input_sequence: 3,
        },
        LegacyResetWindow {
            value: 100,
            source: None,
            received_at_epoch: 20,
            input_sequence: 3,
        },
        LegacyPairWindow {
            used_percentage_basis_points: 7_200,
            reset_at_epoch: 100,
            source_summary: None,
            received_at_epoch: 20,
            input_sequence: 3,
        },
    );
    for window in [
        &mut broker_window,
        &mut session_window,
        &mut coherent_statusline_window,
    ] {
        window["used"]
            .as_object_mut()
            .expect("legacy used object")
            .remove("claude_code_version");
        window["reset"]
            .as_object_mut()
            .expect("legacy reset object")
            .remove("claude_code_version");
        window["paired"]
            .as_object_mut()
            .expect("legacy pair object")
            .remove("claude_code_version");
    }
    let account = serde_json::json!({
        "sessions": {
            "bound-session": {
                "model": null,
                "windows": [session_window, coherent_statusline_window],
                "last_observation_received_at_epoch": 20
            }
        },
        "broker_windows": [broker_window, empty_legacy_window()],
        "latest_reset_epochs": [100, null],
        "reset_barriers": [null, null],
        "input_sequence": 3,
        "spend": {
            "latest_record": null,
            "current_period_record": null,
            "previous_period_record": null
        }
    });
    serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "next_monitor_id": 1,
        "next_input_sequence": 3,
        "last_now_epoch": 100,
        "accounts": {"account-1": account},
        "monitors": {},
        "goals": {}
    }))
    .expect("serialize v1 callback-sequence snapshot")
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
fn v4_migration_keeps_reset_high_water_and_marks_missing_fence_unknown() {
    let mut old: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot(4)).expect("parse v4 snapshot fixture");
    old["accounts"]["account-1"]["latest_reset_epochs"] = serde_json::json!([100, null]);
    let old = serde_json::to_vec(&old).expect("serialize v4 fixture");
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let data_dir = temp.path().join("data");
    let (directory, _) = write_snapshot(&data_dir, &old);

    let migrated = load(&directory)
        .expect("load migrated v4 state")
        .expect("snapshot exists");
    let account = &migrated.accounts["account-1"];
    assert_eq!(account.latest_reset_epochs, [Some(100), None]);
    assert_eq!(
        account.reset_admission_fences,
        [
            ResetAdmissionFence::Unknown { after_epoch: 10 },
            ResetAdmissionFence::Unknown { after_epoch: 10 },
        ]
    );
    assert_eq!(account.used_only_admission_sequences, [None, None]);
}

#[test]
fn v4_migration_keeps_pruned_reset_history_and_sticky_barrier_unknown() {
    let mut old: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot(4)).expect("parse v4 snapshot fixture");
    old["next_input_sequence"] = serde_json::json!(7);
    let account = &mut old["accounts"]["account-1"];
    account["input_sequence"] = serde_json::json!(7);
    account["latest_reset_epochs"] = serde_json::json!([1_000, null]);
    account["reset_barriers"] = serde_json::json!([
        {
            "started_at_epoch": 10,
            "prior_reset_at_epoch": 1_000,
            "prior_used_percentage_basis_points": 9_700,
            "input_sequence_before": 7,
            "source": "broker_projection",
            "session_id": null,
            "pause_reason": "limit_guard_reached"
        },
        null
    ]);
    let account_object = account.as_object().expect("v4 account object");
    assert!(!account_object.contains_key("reset_admission_fences"));
    assert!(!account_object.contains_key("used_only_admission_sequences"));
    assert_eq!(account["sessions"], serde_json::json!({}));
    let old = serde_json::to_vec(&old).expect("serialize v4 fixture");
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let data_dir = temp.path().join("data");
    let (directory, _) = write_snapshot(&data_dir, &old);

    let migrated = load(&directory)
        .expect("load migrated v4 state")
        .expect("snapshot exists");
    let account = &migrated.accounts["account-1"];
    assert_eq!(account.latest_reset_epochs, [Some(1_000), None]);
    assert_eq!(account.input_sequence, 7);
    assert_eq!(
        account.reset_barriers[0]
            .as_ref()
            .map(|barrier| (barrier.prior_reset_at_epoch, barrier.input_sequence_before)),
        Some((Some(1_000), 7))
    );
    assert_eq!(
        account.reset_admission_fences[0],
        ResetAdmissionFence::Unknown { after_epoch: 10 }
    );
    assert!(account.sessions.is_empty());
}

#[test]
fn v2_through_v4_migrations_normalize_only_unsupported_pair_authority() {
    for version in [2, 3, 4] {
        let bytes = legacy_pair_migration_snapshot(version);
        let migrated = if version == 4 {
            migrate_v4(&bytes)
        } else {
            migrate_v2_or_v3(&bytes, version)
        }
        .expect("migrate callback-sequence pair shapes");
        assert_eq!(migrated.schema_version, USAGE_MONITOR_SCHEMA_VERSION);
        let account = &migrated.accounts["account-1"];
        assert_eq!(account.latest_reset_epochs, [Some(200), Some(100)]);
        assert_eq!(
            account.reset_admission_fences[0],
            ResetAdmissionFence::Unknown { after_epoch: 100 }
        );

        let broker = &account.broker_windows[0];
        assert!(broker.paired.is_none());
        assert_eq!(broker.used.as_ref().map(|used| used.value), Some(5_400));
        assert_eq!(broker.reset.as_ref().map(|reset| reset.value), Some(200));
        assert_eq!(
            broker.used.as_ref().map(|used| used.reset_at_epoch),
            Some(Some(100))
        );

        let coherent_broker = &account.broker_windows[1];
        let broker_pair = super::super::resetless::exact_observed_pair(coherent_broker)
            .expect("coherent unequal-source broker aggregate retains provenance");
        assert_eq!(broker_pair.source_times, [Some(10), Some(11)]);
        assert_eq!(broker_pair.evidence_at_epoch, Some(10));

        let session = &account.sessions["bound-session"];
        let reset_only = &session.windows[0];
        assert!(reset_only.paired.is_none());
        assert_eq!(reset_only.used.as_ref().map(|used| used.value), Some(5_400));
        assert_eq!(
            reset_only.reset.as_ref().map(|reset| reset.value),
            Some(200)
        );
        let complete_update = &session.windows[1];
        assert!(complete_update.paired.is_none());
        assert_eq!(
            complete_update.used.as_ref().map(|used| used.value),
            Some(6_000)
        );
        assert_eq!(
            complete_update.reset.as_ref().map(|reset| reset.value),
            Some(100)
        );
        assert_eq!(
            complete_update
                .reset
                .as_ref()
                .map(|reset| reset.input_sequence),
            Some(1)
        );

        let source_shape_mismatch = &account.sessions["source-shape-mismatch"].windows[0];
        assert!(source_shape_mismatch.paired.is_none());
        assert_eq!(
            source_shape_mismatch.used.as_ref().map(|used| used.value),
            Some(7_200)
        );
        assert_eq!(
            source_shape_mismatch
                .reset
                .as_ref()
                .map(|reset| reset.value),
            Some(100)
        );

        let receipt_only_broker =
            &migrated.accounts["account-broker-source-shape"].broker_windows[0];
        assert!(receipt_only_broker.paired.is_none());
        assert_eq!(
            receipt_only_broker.used.as_ref().map(|used| used.value),
            Some(7_200)
        );
        assert_eq!(
            receipt_only_broker.reset.as_ref().map(|reset| reset.value),
            Some(100)
        );

        let unbound = &migrated.unbound_sessions["unbound-session"].windows[1];
        let pair = super::super::resetless::exact_observed_pair(unbound)
            .expect("coherent receipt-only statusline pair keeps provenance");
        assert_eq!(pair.source_times, [None, None]);
        assert_eq!(pair.evidence_at_epoch, None);
        assert_eq!(unbound.used.as_ref().map(|used| used.value), Some(7_200));
        assert_eq!(unbound.reset.as_ref().map(|reset| reset.value), Some(100));
    }
}

#[test]
fn v1_migration_copies_coherent_source_times_and_clears_stale_session_pair() {
    let migrated = super::super::legacy::migrate_v1(&legacy_v1_pair_snapshot())
        .expect("migrate v1 callback-sequence pair shapes");
    let account = &migrated.accounts["account-1"];
    assert_eq!(
        account.reset_admission_fences[0],
        ResetAdmissionFence::Unknown { after_epoch: 100 }
    );
    let broker = &account.broker_windows[0];
    let pair = super::super::resetless::exact_observed_pair(broker)
        .expect("coherent v1 aggregate retains exact source tuple");
    assert_eq!(pair.source_times, [Some(10), Some(11)]);
    assert_eq!(pair.evidence_at_epoch, Some(10));

    let session = &account.sessions["bound-session"].windows[0];
    assert!(session.paired.is_none());
    assert_eq!(session.used.as_ref().map(|used| used.value), Some(6_000));
    assert_eq!(session.reset.as_ref().map(|reset| reset.value), Some(100));
    assert_eq!(
        session.reset.as_ref().map(|reset| reset.input_sequence),
        Some(1)
    );

    let coherent_statusline = &account.sessions["bound-session"].windows[1];
    let pair = super::super::resetless::exact_observed_pair(coherent_statusline)
        .expect("coherent receipt-only statusline pair retains provenance");
    assert_eq!(pair.source_times, [None, None]);
    assert_eq!(pair.evidence_at_epoch, None);
}

#[test]
fn malformed_legacy_pairs_are_rejected_without_rewriting_source_bytes() {
    let mut snapshots = vec![(1, legacy_v1_pair_snapshot())];
    snapshots.extend(
        [2, 3, 4]
            .into_iter()
            .map(|version| (version, legacy_pair_migration_snapshot(version))),
    );
    for (version, bytes) in snapshots {
        let mut malformed: serde_json::Value =
            serde_json::from_slice(&bytes).expect("parse legacy pair snapshot");
        malformed["accounts"]["account-1"]["broker_windows"][0]["paired"]["used_percentage_basis_points"] =
            serde_json::json!(10_001);
        let malformed = serde_json::to_vec(&malformed).expect("serialize malformed pair");
        let temp = tempfile::tempdir().expect("malformed snapshot root");
        let data_dir = temp.path().join(format!("data-v{version}"));
        let (directory, path) = write_snapshot(&data_dir, &malformed);
        load(&directory).expect_err("malformed legacy pair must fail closed");
        assert_eq!(
            std::fs::read(path).expect("read rejected source bytes"),
            malformed,
            "failed version {version} migration must not rewrite source"
        );
    }
}

#[test]
fn unknown_legacy_pair_fields_are_rejected_without_rewriting_source_bytes() {
    for version in [2, 3, 4] {
        let mut malformed: serde_json::Value =
            serde_json::from_slice(&legacy_pair_migration_snapshot(version))
                .expect("parse valid legacy pair snapshot");
        malformed["accounts"]["account-broker-source-shape"]["broker_windows"][0]["paired"]["source_times"] =
            serde_json::json!([null, null]);
        let malformed = serde_json::to_vec(&malformed).expect("serialize unknown pair field");
        let temp = tempfile::tempdir().expect("unknown-field snapshot root");
        let data_dir = temp.path().join(format!("unknown-pair-field-v{version}"));
        let (directory, path) = write_snapshot(&data_dir, &malformed);
        load(&directory).expect_err("unknown legacy pair field must fail closed");
        assert_eq!(
            std::fs::read(path).expect("read rejected source bytes"),
            malformed,
            "unknown-field version {version} rejection must not rewrite source"
        );
    }
}

#[test]
fn admitted_resetless_marker_and_fence_survive_atomic_save_and_reopen() {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let data_dir = temp.path().join("data");
    let directory = open_store_dir(&data_dir).expect("open private store directory");
    let mut state = StoreState {
        last_now_epoch: 102,
        next_input_sequence: 2,
        ..StoreState::default()
    };
    let mut account = AccountObservations {
        input_sequence: 2,
        latest_reset_epochs: [Some(1_000), None],
        reset_admission_fences: [
            ResetAdmissionFence::Known {
                reset_at_epoch: 1_000,
                source_after_epoch: 100,
                receipt_after_epoch: 100,
                input_sequence: 1,
            },
            ResetAdmissionFence::KnownNone,
        ],
        used_only_admission_sequences: [Some(2), None],
        ..AccountObservations::default()
    };
    account.broker_windows[0] = ObservedWindow {
        used: Some(ObservedPercentage {
            value: 0,
            reset_at_epoch: None,
            evidence_at_epoch: Some(101),
            received_at_epoch: 102,
            input_sequence: 2,
            claude_code_version: None,
        }),
        reset: Some(Observed {
            value: 1_000,
            evidence_at_epoch: Some(99),
            received_at_epoch: 99,
            input_sequence: 1,
            claude_code_version: None,
        }),
        ..ObservedWindow::default()
    };
    state.accounts.insert("account-1".to_owned(), account);
    let before = &state.accounts["account-1"];
    assert!(super::super::resetless::broker_orphan_is_admissible(
        before,
        0,
        state.last_now_epoch
    ));
    save(&directory, &state).expect("atomically save admitted resetless state");

    let reopened = load(&directory)
        .expect("reopen monitor state")
        .expect("saved state exists");
    let account = &reopened.accounts["account-1"];
    assert_eq!(account.used_only_admission_sequences[0], Some(2));
    assert_eq!(
        account.broker_windows[0].reset.as_ref().map(|reset| (
            reset.value,
            reset.evidence_at_epoch,
            reset.received_at_epoch,
            reset.input_sequence
        )),
        Some((1_000, Some(99), 99, 1)),
        "the independently retained reset remains byte-for-byte represented after reopen"
    );
    assert_eq!(
        account.reset_admission_fences[0],
        before.reset_admission_fences[0]
    );
    assert_eq!(
        account.reset_admission_fences[1],
        ResetAdmissionFence::KnownNone
    );
    assert!(super::super::resetless::broker_orphan_is_admissible(
        account,
        0,
        reopened.last_now_epoch
    ));
}

#[test]
fn save_failures_before_and_after_rename_never_publish_partial_json() {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let data_dir = temp.path().join("data");
    let (directory, path) = write_snapshot(
        &data_dir,
        &serde_json::to_vec(&StoreState {
            last_now_epoch: 10,
            ..StoreState::default()
        })
        .expect("serialize initial state"),
    );
    let original = std::fs::read(&path).expect("read initial state");
    let next = StoreState {
        last_now_epoch: 11,
        ..StoreState::default()
    };

    set_test_save_failure_point(SaveFailurePoint::BeforeRename);
    assert!(save(&directory, &next).is_err());
    assert_eq!(
        std::fs::read(&path).expect("read unchanged published state"),
        original,
        "pre-rename failure leaves the previous snapshot intact"
    );
    assert_eq!(
        std::fs::read_dir(path.parent().expect("store directory"))
            .expect("read store directory")
            .count(),
        1,
        "pre-rename failure cleans the staging file"
    );

    set_test_save_failure_point(SaveFailurePoint::AfterRename);
    assert!(save(&directory, &next).is_err());
    let published = std::fs::read(&path).expect("read complete renamed state");
    let parsed: StoreState =
        serde_json::from_slice(&published).expect("post-rename file is complete JSON");
    assert_eq!(parsed.last_now_epoch, 11);
    assert_eq!(
        std::fs::read_dir(path.parent().expect("store directory"))
            .expect("read store directory")
            .count(),
        1,
        "post-rename failure leaves no staging file"
    );
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
    let rewritten = std::fs::read(&path).expect("read published v5 state");
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
    future["schema_version"] = serde_json::json!(6);
    let future = serde_json::to_vec(&future).expect("serialize future fixture");
    let (future_directory, future_path) = write_snapshot(&future_data_dir, &future);
    load(&future_directory).unwrap_err();
    assert_eq!(
        std::fs::read(&future_path).expect("future source remains untouched"),
        future
    );

    let schema7_temp = tempfile::tempdir().expect("schema 7 fixture root");
    let schema7_data_dir = schema7_temp.path().join("data");
    let mut schema7: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot(3)).expect("parse legacy fixture");
    schema7["schema_version"] = serde_json::json!(7);
    let schema7 = serde_json::to_vec(&schema7).expect("serialize schema 7 fixture");
    let (schema7_directory, schema7_path) = write_snapshot(&schema7_data_dir, &schema7);
    load(&schema7_directory).unwrap_err();
    assert_eq!(
        std::fs::read(&schema7_path).expect("schema 7 source remains untouched"),
        schema7
    );
}

#[test]
fn load_rejects_and_preserves_v3_policy_history_that_loosens_a_strict_budget() {
    let prepared =
        insert_resetless_metadata(&legacy_snapshot(3), 3).expect("prepare the strict v3 shape");
    let mut state: StoreState =
        serde_json::from_slice(&prepared).expect("parse v3 snapshot fixture");
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
    let mut value = serde_json::to_value(state).expect("serialize invalid old-schema fixture");
    let account = value["accounts"]["account-1"]
        .as_object_mut()
        .expect("account observations");
    account.remove("reset_admission_fences");
    account.remove("used_only_admission_sequences");
    account
        .get_mut("spend")
        .and_then(serde_json::Value::as_object_mut)
        .expect("spend account")
        .remove("historical_correction_horizon_epoch");
    value["bindings"]["binding-00000001"][0]
        .as_object_mut()
        .expect("binding")
        .remove("provider_account_id");
    value["bindings"]["binding-00000001"][0]
        .as_object_mut()
        .expect("binding")
        .remove("experimental_collector_approved");
    let bytes = serde_json::to_vec(&value).expect("serialize strict v3 fixture");
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
        "invalid policy history must not be restamped as schema 5"
    );
}
