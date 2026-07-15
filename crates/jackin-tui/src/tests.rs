// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::components::{ConfirmState, confirm_hint_spans, render_confirm_dialog};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn component_contract_example_compiles() {
    let mut state = ConfirmState::new("Proceed?");
    match state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) {
        ModalOutcome::Commit(_) | ModalOutcome::Cancel | ModalOutcome::Continue => {}
    }

    let _render: fn(&mut ratatui::Frame<'_>, ratatui::layout::Rect, &ConfirmState) =
        render_confirm_dialog;
    let _hints = confirm_hint_spans();
}

#[test]
fn text_field_insert_appends() {
    let mut f = TextField::new("");
    f.insert_char('a');
    f.insert_char('b');
    assert_eq!(f.value(), "ab");
    assert_eq!(f.cursor(), 2);
}

#[test]
fn text_field_backspace_removes_one_char() {
    let mut f = TextField::new("abc");
    f.backspace();
    assert_eq!(f.value(), "ab");
}

#[test]
fn text_field_max_chars_caps_buffer() {
    let mut f = TextField::new("").with_max_chars(2);
    f.insert_char('a');
    f.insert_char('b');
    f.insert_char('c');
    assert_eq!(f.value(), "ab");
}

#[test]
fn text_field_duplicate_detection_trims() {
    let f = TextField::new("  foo  ").with_forbidden(vec!["foo".into()]);
    assert!(f.is_duplicate());
}

#[test]
fn text_field_is_valid_requires_non_empty_by_default() {
    let f = TextField::new("");
    assert!(!f.is_valid());
    let f = f.with_allow_empty(true);
    assert!(f.is_valid());
}

#[test]
fn shorten_home_returns_path_when_no_match() {
    // Use the actual `HOME` from the test environment without
    // mutating it. Anything not starting with `$HOME` is
    // returned unchanged, which is the only branch we can
    // verify reliably without an `unsafe` env-var write (the
    // crate's lints forbid `unsafe`).
    let home = std::env::var("HOME").unwrap_or_default();
    let alien = if home == "/" {
        "etc/hosts".to_owned()
    } else {
        format!("{home}.notmine")
    };
    assert_eq!(shorten_home(&alien), alien);
}

#[test]
fn text_field_control_chars_are_ignored() {
    let mut f = TextField::new("");
    f.insert_char('\n');
    f.insert_char('\x1b');
    assert!(f.is_empty());
}

#[test]
fn lay_out_tabs_packs_cells_with_single_gap() {
    let cells = lay_out_tabs(&[("General", true), ("Mounts", false)], 0);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].start_col, 0);
    assert_eq!(cells[0].cell_cols, 9); // " General "
    assert!(cells[0].active);
    // Second tab starts after first cell + single-column gap.
    assert_eq!(cells[1].start_col, 9 + 1);
    assert_eq!(cells[1].cell_cols, 8); // " Mounts "
    assert!(!cells[1].active);
}

#[test]
fn hint_span_display_cols_match_render_contract() {
    // Key spans render the glyph(s) unchanged.
    assert_eq!(HintSpan::Key("↵").display_cols(), 1);
    // Text spans render with a leading space.
    assert_eq!(HintSpan::Text("save").display_cols(), 5);
    // Separators occupy three columns each.
    assert_eq!(HintSpan::Sep.display_cols(), 3);
    assert_eq!(HintSpan::GroupSep.display_cols(), 3);
    // DynKey renders like Key — no leading space, char count.
    assert_eq!(HintSpan::DynKey("^\\".to_owned()).display_cols(), 2);
    // Multi-byte / wide glyphs use char count, not byte len.
    assert_eq!(HintSpan::Key("↑↓").display_cols(), 2);
}

#[test]
fn hint_row_cols_sums_spans() {
    let spans = [
        HintSpan::Key("↵"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ];
    assert_eq!(hint_row_cols(&spans), 1 + 5 + 3 + 3 + 7);
}

#[test]
fn hint_row_cols_handles_empty_slice() {
    assert_eq!(hint_row_cols(&[]), 0);
}

#[test]
fn encode_osc52_clipboard_write_uses_bel_terminated_base64_framing() {
    // The exact byte sequence terminals parse for OSC 52: `\x1b]52;c;` +
    // base64 of the payload + BEL. A framing bug here silently copies
    // nothing on the operator's terminal.
    use base64::Engine as _;
    let payload = "jk-run-42f9aa";
    let bytes = ansi::encode_osc52_clipboard_write(payload);
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
    let mut expected = Vec::new();
    expected.extend_from_slice(b"\x1b]52;c;");
    expected.extend_from_slice(encoded.as_bytes());
    expected.extend_from_slice(b"\x07");
    assert_eq!(bytes, expected);
}

#[test]
fn encode_osc52_clipboard_write_handles_empty_payload() {
    // Empty payloads still produce a well-formed OSC 52 sequence with an
    // empty base64 body; the terminal interprets that as "clear".
    let bytes = ansi::encode_osc52_clipboard_write("");
    assert_eq!(bytes, b"\x1b]52;c;\x07");
}

#[test]
fn pointer_shape_osc22_names_are_shared() {
    assert_eq!(PointerShape::Default.as_osc22_name(), "default");
    assert_eq!(PointerShape::Pointer.as_osc22_name(), "pointer");
    assert_eq!(PointerShape::Text.as_osc22_name(), "text");
    assert_eq!(PointerShape::EwResize.as_osc22_name(), "ew-resize");
    assert_eq!(PointerShape::NsResize.as_osc22_name(), "ns-resize");
    assert_eq!(PointerShape::Grabbing.as_osc22_name(), "grabbing");
    assert_eq!(
        osc22_pointer_shape(PointerShape::Pointer),
        ansi::POINTER_HAND
    );
    assert_eq!(
        osc22_pointer_shape(PointerShape::Default),
        ansi::POINTER_DEFAULT
    );
}

#[test]
fn clickable_pointer_shape_maps_clickability_to_pointer_or_default() {
    assert_eq!(clickable_pointer_shape(true), PointerShape::Pointer);
    assert_eq!(clickable_pointer_shape(false), PointerShape::Default);
}

#[test]
fn take_display_cols_truncates_to_display_width() {
    // ASCII: char count == display width, plain prefix truncation.
    assert_eq!(take_display_cols("abcdef", 3), "abc");
    // Wide chars (CJK, width 2) must not be split mid-character: with a
    // 3-col budget after `a` (1) we have 2 cols left, which fits one wide
    // char (2) but not two.
    assert_eq!(take_display_cols("a日本", 3), "a日");
    // Control bytes are skipped, not counted.
    assert_eq!(take_display_cols("a\x07bc", 3), "abc");
}

#[test]
fn take_display_cols_returns_empty_when_budget_is_zero() {
    assert_eq!(take_display_cols("abc", 0), "");
}

#[test]
fn padded_line_display_cols_mirrors_leading_padding() {
    assert_eq!(padded_line_display_cols(["  abc", "日本"]), 2 + 3 + 4 + 2);
}

#[test]
fn leading_space_cols_skips_controls_and_stops_at_text() {
    assert_eq!(leading_space_cols([" \x07 ", "abc", "  "]), 2);
}

#[test]
fn fixed_prefix_scroll_segments_keep_prefix_and_scroll_suffix_by_columns() {
    let segments = fixed_prefix_scroll_segments("▸  a日本z", 0, 3, 1, 8);
    let rendered: Vec<(&str, usize, usize)> = segments
        .iter()
        .map(|seg| {
            (
                &"▸  a日本z"[seg.start_byte..seg.end_byte],
                seg.target_col,
                seg.display_cols,
            )
        })
        .collect();

    assert_eq!(
        rendered,
        vec![
            ("▸", 0, 1),
            (" ", 1, 1),
            (" ", 2, 1),
            ("日", 3, 2),
            ("本", 5, 2),
            ("z", 7, 1)
        ]
    );
}

#[test]
fn fixed_prefix_scroll_segments_keep_combining_mark_with_base() {
    let text = "▸  e\u{301}ab";
    let segments = fixed_prefix_scroll_segments(text, 0, 3, 0, 8);
    let rendered: Vec<&str> = segments
        .iter()
        .map(|seg| &text[seg.start_byte..seg.end_byte])
        .collect();

    assert!(rendered.contains(&"e\u{301}"));
}

#[test]
fn version_splash_has_mark_version_byline_under_six_lines() {
    let s = ansi::version_splash("9.9.9");
    assert!(s.contains("jackin"));
    assert!(s.contains("9.9.9"));
    assert!(s.contains("by tailrocks"));
    // Green block + white chevron (never the same colour as the word).
    assert!(s.contains("\x1b[48;2;0;255;65m"));
    assert!(s.contains("\x1b[38;2;255;255;255m"));
    assert!(s.lines().count() <= 6, "version splash exceeds six lines");
}

#[test]
#[expect(
    clippy::excessive_nesting,
    reason = "Test that walks every char of the banner output verifying \
                  determinism + width + phosphor-prefix — the nested `if ch == ESC` \
                  + `for c in chars` loop is the ANSI-stripping logic the test \
                  depends on."
)]
fn help_banner_is_deterministic_bounded_phosphor() {
    fn visible_cols(line: &str) -> usize {
        let mut n = 0;
        let mut chars = line.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                n += 1;
            }
        }
        n
    }

    let a = ansi::help_banner(80);
    // Deterministic: identical bytes for the same width on every call.
    assert_eq!(a, ansi::help_banner(80));
    // Centered lockup: the green block, the word, and the byline.
    assert!(a.contains("\x1b[48;2;0;255;65m"));
    assert!(a.contains("jackin"));
    assert!(a.contains("by tailrocks"));
    // Phosphor rain present (not a blank field).
    assert!(a.contains("\x1b[38;2;0;255;65m"));
    // Bounded so it never wraps the terminal it was sized for.
    for line in a.lines() {
        assert!(
            visible_cols(line) <= 80,
            "help banner line exceeds terminal width: {line:?}"
        );
    }
}
