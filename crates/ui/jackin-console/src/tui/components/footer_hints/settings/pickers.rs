// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings picker and destination footer items.

use termrock::{keymap::glyph, widgets::HintSpan};

#[must_use]
pub fn mount_destination_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(mount-destination-no-keymap): M handled inline; no MOUNT_DESTINATION_KEYMAP.
        super::super::key_span("M"),
        HintSpan::Text("mount"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(mount-destination-no-keymap): E handled inline.
        super::super::key_span("E"),
        HintSpan::Text("edit"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined left/right display.
        super::super::key_span(glyph::LEFT_RIGHT),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(mount-destination-no-keymap): Enter confirms inline.
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined C/Esc cancel display.
        super::super::key_span("C/Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn segmented_choice_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        super::super::key_span(glyph::LEFT_RIGHT),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(segmented-choice-no-keymap): Enter handled inline; no SEGMENTED_CHOICE_KEYMAP.
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(segmented-choice-no-keymap): Esc handled inline.
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn pick_list_footer_items(commit_label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        super::super::key_span("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(pick-list-no-keymap): Enter handled inline; no PICK_LIST_KEYMAP.
        super::super::key_span("↵"),
        HintSpan::Text(commit_label),
        HintSpan::GroupSep,
        // UNREGISTERABLE(pick-list-no-keymap): Esc handled inline.
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
        // UNREGISTERABLE(multi-key-display-group)
        super::super::key_span("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(descriptive-label): not a key — describes free-text filter input.
        super::super::key_span("type"),
        HintSpan::Text("filter"),
    ];
    if include_refresh {
        items.extend([
            HintSpan::GroupSep,
            // UNREGISTERABLE(filtered-picker-no-keymap): R refresh handled inline; no FILTERED_PICKER_KEYMAP.
            super::super::key_span("R"),
            HintSpan::Text("refresh"),
        ]);
    }
    if include_collapse {
        items.extend([
            HintSpan::GroupSep,
            // UNREGISTERABLE(multi-key-display-group)
            super::super::key_span(glyph::LEFT_RIGHT),
            HintSpan::Text("collapse/expand section"),
        ]);
    }
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(filtered-picker-no-keymap): Enter selects inline.
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(filtered-picker-no-keymap): Esc cancels inline.
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

#[must_use]
pub fn op_section_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        super::super::key_span("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(op-section-no-keymap): Enter handled inline; no OP_SECTION_KEYMAP.
        super::super::key_span("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(op-section-no-keymap): Esc handled inline.
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ]
}
