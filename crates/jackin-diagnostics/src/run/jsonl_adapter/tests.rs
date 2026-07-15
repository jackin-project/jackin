// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{PROHIBITED_TOP_LEVEL_KEYS, SCHEMA_V2, canonicalize_line, has_no_prohibited_keys};
use serde_json::json;

#[test]
fn v1_line_maps_kind_stage_error_type() {
    let line = r#"{"ts_ms":1,"run_id":"r1","kind":"stage_started","stage":"image","message":"go","error_type":"E1"}"#;
    let event = canonicalize_line(line).unwrap();
    assert_eq!(event.schema, 1);
    assert_eq!(event.kind, "stage_started");
    assert_eq!(event.event_name, "launch.stage.started");
    assert_eq!(event.stage.as_deref(), Some("image"));
    assert_eq!(event.error_type.as_deref(), Some("E1"));
    assert_eq!(event.run_id.as_deref(), Some("r1"));
}

#[test]
fn v1_expected_shutdown_becomes_expected_close() {
    let line = r#"{"kind":"session_detach","event.outcome":"expected_shutdown","message":"bye"}"#;
    let event = canonicalize_line(line).unwrap();
    assert_eq!(event.event_outcome.as_deref(), Some("expected_close"));
    assert_eq!(event.event_name, "capsule.session.detach");
}

#[test]
fn v2_line_uses_canonical_keys_and_kind_from_registry() {
    let line = r#"{"schema":2,"ts_ms":2,"parallax.run.id":"r2","event.name":"launch.stage.done","jackin.stage":"image","jackin.detail":"{\"duration_ms\":10}","message":"built"}"#;
    let event = canonicalize_line(line).unwrap();
    assert_eq!(event.schema, SCHEMA_V2);
    assert_eq!(event.kind, "stage_done");
    assert_eq!(event.event_name, "launch.stage.done");
    assert_eq!(event.stage.as_deref(), Some("image"));
    assert_eq!(event.detail.as_deref(), Some(r#"{"duration_ms":10}"#));
    assert_eq!(event.run_id.as_deref(), Some("r2"));
}

#[test]
fn prohibited_keys_list_is_complete() {
    assert!(PROHIBITED_TOP_LEVEL_KEYS.contains(&"kind"));
    assert!(PROHIBITED_TOP_LEVEL_KEYS.contains(&"run_id"));
    let clean = json!({
        "schema": 2,
        "parallax.run.id": "r",
        "event.name": "capsule.log",
        "message": "ok"
    });
    assert!(has_no_prohibited_keys(&clean));
    let dirty = json!({"schema": 2, "kind": "x", "message": "no"});
    assert!(!has_no_prohibited_keys(&dirty));
}

#[test]
fn live_writer_emits_schema_v2_without_prohibited() {
    use jackin_core::JackinPaths;
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = crate::RunDiagnostics::start(&paths, true, "load").unwrap();
    run.compact("breadcrumb", "hello v2");
    run.flush_writer();
    let contents = std::fs::read_to_string(run.path()).unwrap();
    for line in contents.lines().filter(|l| !l.trim().is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["schema"], 2);
        assert!(
            has_no_prohibited_keys(&value),
            "prohibited keys present: {line}"
        );
        assert!(value.get("parallax.run.id").is_some());
        assert!(value.get("event.name").is_some());
    }
}
