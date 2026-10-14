// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn main_view_hint_omits_scroll_when_focused_pane_fits() {
    let hint = labels(&main_view_hint(
        false,
        0x1C,
        termrock::scroll::ScrollAxes::default(),
        false,
    ));
    assert!(hint.contains("Ctrl-\\ menu"), "hint: {hint}");
    assert!(hint.contains("click focus pane"), "hint: {hint}");
    assert!(
        !hint.contains("scroll"),
        "fit-content main view must not advertise scroll: {hint}"
    );
}

#[test]
fn usage_hint_names_provider_switch_focus_refresh_and_close() {
    let hint = labels(&usage_hint(termrock::scroll::ScrollAxes {
        vertical: true,
        horizontal: false,
    }));

    assert!(hint.contains("←→ switch provider"), "hint: {hint}");
    assert!(
        hint.contains(&format!("{} focus content", glyph::TAB)),
        "hint: {hint}"
    );
    assert!(hint.contains("r refresh"), "hint: {hint}");
    assert!(hint.contains("↑↓/j/k scroll"), "hint: {hint}");
    assert!(hint.contains("Esc close"), "hint: {hint}");
    assert!(
        !hint.contains("copy"),
        "usage overlay must not reuse copy-dialog hints: {hint}"
    );
}

#[test]
fn main_view_hint_advertises_only_visible_scroll_axis() {
    let hint = labels(&main_view_hint(
        false,
        0x1C,
        termrock::scroll::ScrollAxes {
            vertical: true,
            horizontal: false,
        },
        false,
    ));
    assert!(hint.contains("↑↓/j/k scroll"), "hint: {hint}");
    assert!(hint.contains("click focus pane"), "hint: {hint}");
    assert!(
        !hint.contains("←→/h/l scroll"),
        "vertical-only pane must not advertise horizontal scroll: {hint}"
    );
}

#[test]
fn scrollback_hint_omits_scroll_when_no_axis_is_visible() {
    let hint = labels(&main_view_hint(
        true,
        0x1C,
        termrock::scroll::ScrollAxes::default(),
        false,
    ));
    assert!(hint.contains("Esc exit scrollback"), "hint: {hint}");
    assert!(hint.contains("Ctrl-\\ menu"), "hint: {hint}");
    assert!(
        !hint.contains("↑↓ scroll"),
        "scrollback exit hint must not advertise scroll without a visible axis: {hint}"
    );
}

#[test]
fn prefix_awaiting_shows_cheat_sheet_not_nav_hints() {
    let hint = labels(&main_view_hint(
        false,
        0x1C,
        termrock::scroll::ScrollAxes::default(),
        true,
    ));
    assert!(
        hint.contains("Ctrl-\\ palette"),
        "must advertise palette: {hint}"
    );
    assert!(hint.contains("h/j/k/l nav"), "must show nav group: {hint}");
    assert!(hint.contains("Ctrl-Q quit"), "must show quit: {hint}");
    assert!(
        !hint.contains("click"),
        "prefix hint must not show mouse nav: {hint}"
    );
}

#[test]
fn main_view_hint_includes_resize_pane_group() {
    let hint = labels(&main_view_hint(
        false,
        0x1C,
        termrock::scroll::ScrollAxes::default(),
        false,
    ));
    assert!(
        hint.contains("Alt-Shift-↑↓←→ resize pane"),
        "live main view must advertise pane resize gesture: {hint}"
    );
}

#[test]
fn scrollback_hint_does_not_include_resize_pane() {
    let hint = labels(&main_view_hint(
        true,
        0x1C,
        termrock::scroll::ScrollAxes::default(),
        false,
    ));
    assert!(
        !hint.contains("resize pane"),
        "scrollback hint must not advertise pane resize: {hint}"
    );
}

#[test]
fn custom_palette_key_glyph_appears_in_hint() {
    let hint = labels(&main_view_hint(
        false,
        0x05,
        termrock::scroll::ScrollAxes::default(),
        false,
    ));
    assert!(
        hint.contains("Ctrl-E menu"),
        "custom palette key must appear with Ctrl- prefix: {hint}"
    );
    assert!(
        !hint.contains("Ctrl-\\ menu"),
        "default glyph must not appear when key is overridden: {hint}"
    );
}

#[test]
fn format_key_glyph_ctrl_backslash() {
    assert_eq!(format_key_glyph(0x1C), "Ctrl-\\");
}

#[test]
fn format_key_glyph_ctrl_e() {
    assert_eq!(format_key_glyph(0x05), "Ctrl-E");
}
