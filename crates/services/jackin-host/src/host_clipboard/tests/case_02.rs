// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn linux_clipboard_backend_reports_missing_display_bridge() {
    let err = validate_linux_clipboard_backend(
        false,
        false,
        false,
        false,
        "Linux host clipboard image reader",
    )
    .expect_err("missing display bridge should explain setup");

    assert!(format!("{err:#}").contains("WAYLAND_DISPLAY with wl-paste or DISPLAY with xclip"));
}

#[test]
fn linux_clipboard_backend_reports_missing_wayland_tool() {
    let err = validate_linux_clipboard_backend(
        true,
        false,
        false,
        false,
        "Linux host clipboard image reader",
    )
    .expect_err("missing wl-paste should explain setup");

    assert!(format!("{err:#}").contains("needs wl-paste in host PATH"));
}

#[test]
fn linux_clipboard_backend_reports_missing_x11_tool() {
    let err = validate_linux_clipboard_backend(
        false,
        true,
        false,
        false,
        "Linux host clipboard image reader",
    )
    .expect_err("missing xclip should explain setup");

    assert!(format!("{err:#}").contains("needs xclip in host PATH"));
}

#[test]
fn linux_clipboard_backend_reports_both_tools_missing_when_both_servers_set() {
    let err = validate_linux_clipboard_backend(
        true,
        true,
        false,
        false,
        "Linux host clipboard image reader",
    )
    .expect_err("both servers set but neither tool present should explain setup");

    assert!(format!("{err:#}").contains("needs wl-paste or xclip in host PATH"));
}

#[test]
fn linux_clipboard_backend_accepts_any_available_display_tool_pair() {
    validate_linux_clipboard_backend(
        true,
        false,
        true,
        false,
        "Linux host clipboard image reader",
    )
    .expect("Wayland with wl-paste should work");
    validate_linux_clipboard_backend(
        false,
        true,
        false,
        true,
        "Linux host clipboard image reader",
    )
    .expect("X11 with xclip should work");
    // Both servers advertised, only one tool present → still accepted.
    validate_linux_clipboard_backend(true, true, true, false, "Linux host clipboard image reader")
        .expect("both servers with only wl-paste should work");
}
