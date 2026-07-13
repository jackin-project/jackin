// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{RunDiagnostics, slow_foreground_wait_payload};

#[test]
fn no_payload_at_or_below_threshold() {
    assert!(slow_foreground_wait_payload("image_decision", 500, 500).is_none());
    assert!(slow_foreground_wait_payload("image_decision", 0, 500).is_none());
}

#[test]
fn payload_explains_wait_over_threshold() {
    let wait =
        slow_foreground_wait_payload("docker_build", 1234, 500).expect("over threshold → payload");
    assert!(
        wait.message.contains("docker_build")
            && wait.message.contains("1234ms")
            && wait.message.contains("500ms"),
        "message names the stage and both durations: {}",
        wait.message
    );
    // Assert the parsed fields, not byte layout: serde_json key order depends on
    // whether the `preserve_order` feature is unified in (sorted vs. insertion).
    let parsed: serde_json::Value =
        serde_json::from_str(&wait.detail).expect("detail is valid JSON");
    assert_eq!(parsed["label"], "docker_build");
    assert_eq!(parsed["duration_ms"], 1234);
    assert_eq!(parsed["threshold_ms"], 500);
}

#[test]
fn threshold_constant_is_500ms() {
    assert_eq!(RunDiagnostics::FOREGROUND_WAIT_EXPLAIN_THRESHOLD_MS, 500);
}
