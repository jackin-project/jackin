// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn assert_shown_glyphs_are_normalized<A: Copy + 'static>(
    keymap: &termrock::input::Keymap<A>,
) {
    for span in keymap.hint_spans() {
        let termrock::widgets::HintSpan::Key(key) = span else {
            continue;
        };
        assert_ne!(key, concat!("T", "ab"));
        assert!(!key.contains("\u{2191}/"));
        assert!(!key.contains("\u{2190}/"));
        assert!(!key.contains(concat!("PgUp", " PgDn")));
        assert!(!key.contains(concat!("Alt", "+")));
        assert!(!key.contains(concat!("Shift", "+")));
        assert!(!key.contains(concat!("Ctrl", "+")));
    }
}
