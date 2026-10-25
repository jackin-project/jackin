// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{UsageBridgeError, UsageMenuBarBridge};

#[test]
fn unopened_bridge_reports_runtime_unavailable() {
    let bridge = UsageMenuBarBridge::create();
    assert_eq!(
        bridge.list_surfaces().expect_err("runtime is not open"),
        UsageBridgeError::RuntimeUnavailable
    );
}

#[test]
fn unknown_surface_is_rejected_without_touching_runtime() {
    let bridge = UsageMenuBarBridge::create();
    assert_eq!(
        bridge
            .set_enabled("definitely-not-a-surface".to_owned(), true)
            .expect_err("surface is unknown"),
        UsageBridgeError::rejected("runtime", "unknown surface")
    );
}

#[test]
fn bridge_debug_shape_does_not_leak_internals() {
    let bridge = UsageMenuBarBridge::create();
    let rendered = format!("{bridge:?}");
    assert!(rendered.contains(".."), "{rendered}");
    for leaked in ["inner", "lifecycle", "Mutex", "BridgeState"] {
        assert!(!rendered.contains(leaked), "{rendered}");
    }
}
