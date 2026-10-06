// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dialog hint-span builders.

use termrock::widgets::HintSpan;

#[must_use]
pub fn text_input_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn confirm_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵"),
        HintSpan::Text("confirm"),
        HintSpan::GroupSep,
        HintSpan::Key("Y/N"),
        HintSpan::Text("choose"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn error_popup_hint_spans() -> Vec<HintSpan<'static>> {
    vec![HintSpan::Key("↵/Esc"), HintSpan::Text("dismiss")]
}

#[must_use]
pub fn save_discard_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("D"),
        HintSpan::Text("discard"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}
