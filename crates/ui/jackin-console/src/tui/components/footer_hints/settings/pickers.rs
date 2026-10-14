// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::{keymap::glyph, widgets::HintSpan};

#[must_use]
pub fn mount_destination_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        super::super::key_span("M"),
        HintSpan::Text("mount"),
        HintSpan::GroupSep,
        super::super::key_span("E"),
        HintSpan::Text("edit"),
        HintSpan::GroupSep,
        super::super::key_span(glyph::LEFT_RIGHT),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        super::super::key_span("C/Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn segmented_choice_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        super::super::key_span(glyph::LEFT_RIGHT),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn pick_list_footer_items(commit_label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        super::super::key_span("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        super::super::key_span("↵"),
        HintSpan::Text(commit_label),
        HintSpan::GroupSep,
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn filtered_picker_footer_items(
    include_refresh: bool,
    include_collapse: bool,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        super::super::key_span("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        super::super::key_span("type"),
        HintSpan::Text("filter"),
    ];
    if include_refresh {
        items.extend([
            HintSpan::GroupSep,
            super::super::key_span("R"),
            HintSpan::Text("refresh"),
        ]);
    }
    if include_collapse {
        items.extend([
            HintSpan::GroupSep,
            super::super::key_span(glyph::LEFT_RIGHT),
            HintSpan::Text("collapse/expand section"),
        ]);
    }
    items.extend([
        HintSpan::GroupSep,
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

#[must_use]
pub fn op_section_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        super::super::key_span("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ]
}
