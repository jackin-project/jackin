// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::os::unix::fs::MetadataExt as _;

fn persisted_state_path(directory: &tempfile::TempDir) -> std::path::PathBuf {
    directory
        .path()
        .join(crate::BROKER_DIR)
        .join("monitor")
        .join("state.json")
}

fn populated_monitor_state() -> (tempfile::TempDir, String, serde_json::Value) {
    let (directory, store) = open_store();
    let session_id = "session-v2-event-migration";
    let started = start_result(
        &store,
        observe_config(session_id),
        "v2-event-migration-start",
        NOW,
    )
    .expect("start a valid observation-only monitor");
    ingest(
        &store,
        MonitorScope::Session {
            session_id: session_id.to_owned(),
        },
        observation(
            session_id,
            Some("claude-sonnet"),
            Some("2.1.81"),
            quota(1_000, NOW + 10_000),
            quota(2_000, NOW + 20_000),
        ),
        NOW + 1,
    );
    store
        .operate(
            MonitorOperation::Stop {
                monitor_id: started.monitor_id.clone(),
            },
            NOW + 2,
        )
        .expect("persist a monitor event");
    drop(store);

    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(persisted_state_path(&directory)).expect("read populated monitor state"),
    )
    .expect("parse populated monitor state");
    assert!(
        !events_for(&state, &started.monitor_id).is_empty(),
        "fixture must contain a persisted monitor event"
    );
    (directory, started.monitor_id, state)
}

fn events_for<'a>(state: &'a serde_json::Value, monitor_id: &str) -> &'a [serde_json::Value] {
    state["monitors"][monitor_id]["events"]
        .as_array()
        .expect("monitor event array")
        .as_slice()
}

fn set_event_schema_versions(
    state: &mut serde_json::Value,
    monitor_id: &str,
    version: u16,
) -> usize {
    let events = state["monitors"][monitor_id]["events"]
        .as_array_mut()
        .expect("monitor event array");
    assert!(!events.is_empty(), "fixture must contain a monitor event");
    for event in events.iter_mut() {
        event["status"]["schema_version"] = serde_json::json!(version);
    }
    events.len()
}

fn write_state_fixture(directory: &tempfile::TempDir, state: &serde_json::Value) -> Vec<u8> {
    let path = persisted_state_path(directory);
    assert_eq!(
        std::fs::metadata(&path)
            .expect("monitor state metadata")
            .mode()
            & 0o777,
        0o600,
        "monitor fixture stays owner-only"
    );
    let bytes = serde_json::to_vec(state).expect("serialize monitor fixture");
    std::fs::write(&path, &bytes).expect("install monitor fixture");
    assert_eq!(
        std::fs::metadata(&path)
            .expect("installed monitor state metadata")
            .mode()
            & 0o777,
        0o600,
        "monitor fixture stays owner-only"
    );
    bytes
}

fn assert_rejected_without_rewriting(directory: &tempfile::TempDir, bytes: &[u8]) {
    let path = persisted_state_path(directory);
    assert_eq!(
        std::fs::read(&path).expect("read fixture before opening"),
        bytes
    );
    for _ in 0..2 {
        let error = MonitorStore::open(directory.path())
            .expect_err("invalid nested event version must fail closed");
        assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
        assert_eq!(
            std::fs::read(&path).expect("invalid source remains byte-exact"),
            bytes,
            "failed migration must not rewrite its source"
        );
    }
}

#[test]
fn v2_store_rejects_wrong_nested_event_schema_without_rewriting_source() {
    let (directory, monitor_id, mut state) = populated_monitor_state();
    state["schema_version"] = serde_json::json!(2);
    let event_count = set_event_schema_versions(&mut state, &monitor_id, 2);
    state["monitors"][&monitor_id]["events"][0]["status"]["schema_version"] =
        serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION);
    assert_eq!(events_for(&state, &monitor_id).len(), event_count);

    let bytes = write_state_fixture(&directory, &state);
    assert_rejected_without_rewriting(&directory, &bytes);
}

#[test]
fn v3_migration_rejects_wrong_or_future_nested_event_schema_without_rewriting_source() {
    for mismatched_version in [USAGE_MONITOR_SCHEMA_VERSION, u16::MAX] {
        let (directory, monitor_id, mut state) = populated_monitor_state();
        state["schema_version"] = serde_json::json!(3);
        let event_count = set_event_schema_versions(&mut state, &monitor_id, 3);
        state["monitors"][&monitor_id]["events"][0]["status"]["schema_version"] =
            serde_json::json!(mismatched_version);
        assert_eq!(events_for(&state, &monitor_id).len(), event_count);

        let bytes = write_state_fixture(&directory, &state);
        assert_rejected_without_rewriting(&directory, &bytes);
    }
}

#[test]
fn v2_store_migrates_explicitly_to_current_schema() {
    let (directory, monitor_id, mut state) = populated_monitor_state();
    state["schema_version"] = serde_json::json!(2);
    let event_count = set_event_schema_versions(&mut state, &monitor_id, 2);
    assert!(
        event_count > 0,
        "migration control must contain persisted events"
    );
    write_state_fixture(&directory, &state);

    let migrated = MonitorStore::open(directory.path()).expect("migrate populated V2 monitor");
    let rewritten: serde_json::Value = serde_json::from_slice(
        &std::fs::read(persisted_state_path(&directory)).expect("read rewritten monitor state"),
    )
    .expect("parse rewritten monitor state");
    let mut expected = state;
    expected["schema_version"] = serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION);
    set_event_schema_versions(&mut expected, &monitor_id, USAGE_MONITOR_SCHEMA_VERSION);
    assert_eq!(rewritten, expected);
    assert_eq!(
        events_for(&rewritten, &monitor_id).len(),
        event_count,
        "migration preserves every populated event"
    );
    drop(migrated);
}
