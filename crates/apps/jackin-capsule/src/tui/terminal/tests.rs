// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `terminal`.
use super::*;

#[test]
fn client_owned_mode_state_captures_mouse_focus_and_alternate_scroll() {
    let state = client_owned_mode_state();
    for needle in [
        &b"\x1b[?7l"[..],
        &b"\x1b[?1003h"[..],
        &b"\x1b[?1006h"[..],
        &b"\x1b[?1004h"[..],
        &b"\x1b[?1007l"[..],
    ] {
        assert!(
            state.windows(needle.len()).any(|w| w == needle),
            "client_owned_mode_state missing {needle:?}; got {state:?}"
        );
    }
}

#[test]
fn osc22_pointer_shape_uses_css_names() {
    assert_eq!(
        osc22_pointer_shape(PointerShape::Pointer),
        b"\x1b]22;pointer\x1b\\"
    );
    assert_eq!(
        osc22_pointer_shape(PointerShape::EwResize),
        b"\x1b]22;ew-resize\x1b\\"
    );
}

#[test]
fn outer_terminal_reset_disables_alternate_scroll() {
    let reset = outer_terminal_reset_sequence();
    let needle = b"\x1b[?1007l";
    assert!(
        reset.windows(needle.len()).any(|w| w == needle),
        "outer terminal reset missing alternate-scroll disable: {reset:?}"
    );
}

#[test]
fn outer_terminal_reset_restores_autowrap() {
    let reset = outer_terminal_reset_sequence();
    let needle = b"\x1b[?7h";
    assert!(
        reset.windows(needle.len()).any(|w| w == needle),
        "outer terminal reset missing autowrap restore: {reset:?}"
    );
}

#[test]
fn reset_base_excludes_alt_screen_leave() {
    assert!(
        !OUTER_TERMINAL_RESET_BASE
            .windows(ALTERNATE_SCREEN_LEAVE.len())
            .any(|w| w == ALTERNATE_SCREEN_LEAVE),
        "reset base must not contain the alternate-screen leave"
    );
    let mut full = OUTER_TERMINAL_RESET_BASE.to_vec();
    full.extend_from_slice(ALTERNATE_SCREEN_LEAVE);
    assert!(full.ends_with(ALTERNATE_SCREEN_LEAVE));
}

#[test]
fn reset_clear_home_resets_style_before_erasing() {
    assert!(
        RESET_CLEAR_HOME.starts_with(b"\x1b[0m\x1b[2J\x1b[H"),
        "raw attach clears must reset SGR before erase so BCE cannot inherit a pane/tab background"
    );
}

#[test]
fn normalize_size_replaces_zero_dimensions_with_defaults() {
    assert_eq!(normalize_size(0, 0), (DEFAULT_ROWS, DEFAULT_COLS));
}

#[test]
fn normalize_size_clamps_tiny_dimensions_to_pty_safe_floor() {
    assert_eq!(normalize_size(1, 1), (MIN_ROWS, MIN_COLS));
}

#[test]
fn outer_terminal_reset_leads_with_sgr_reset() {
    // The last frame's colors stay asserted on the outer terminal; without
    // a leading SGR reset the host's post-exit output BCE-fills with the
    // final background (the red run-id chip turned the whole screen red).
    let reset = outer_terminal_reset_sequence();
    assert!(
        reset.starts_with(b"\x1b[0m"),
        "outer terminal reset must start with SGR reset: {reset:?}"
    );
}
