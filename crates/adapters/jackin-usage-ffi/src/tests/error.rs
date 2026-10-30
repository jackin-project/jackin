// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::UsageBridgeError;

#[test]
fn rejected_display_joins_code_and_message() {
    let error = UsageBridgeError::rejected("runtime", "unknown surface");
    assert_eq!(error.to_string(), "runtime: unknown surface");
}

#[test]
fn runtime_unavailable_display_names_state() {
    assert_eq!(
        UsageBridgeError::RuntimeUnavailable.to_string(),
        "runtime unavailable"
    );
}

#[test]
fn resync_required_display_names_cursor() {
    assert_eq!(
        UsageBridgeError::ResyncRequired.to_string(),
        "event cursor requires resync"
    );
}

#[test]
fn contained_panic_display_marks_containment() {
    let error = UsageBridgeError::ContainedPanic {
        message: "boom".to_owned(),
    };
    assert_eq!(error.to_string(), "contained panic: boom");
}
