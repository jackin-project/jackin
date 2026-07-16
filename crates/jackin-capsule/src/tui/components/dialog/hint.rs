// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Footer hint rows for capsule dialogs.

use termrock::{keymap::glyph, widgets::HintSpan};

use crate::tui::keymap::{
    CAPSULE_GLOBAL_KEYMAP, FILTER_LIST_KEYMAP, FilterListAction, PREFIX_COMMAND_KEYMAP,
    READ_ONLY_DISMISS_KEYMAP, RENAME_KEYMAP, RESIZE_PANE_KEYMAP, ReadOnlyDismissAction,
    RenameAction,
};

/// Derive a display glyph for a raw palette-key byte.
///
/// Mirrors the `Ctrl-` prefix convention used by [`termrock::keymap::chord_glyph`]
/// so the hint bar is visually consistent regardless of which key the operator
/// configured via `JACKIN_PALETTE_KEY`.
fn format_key_glyph(byte: u8) -> String {
    match byte {
        0x01..=0x1A => format!("Ctrl-{}", (b'@' + byte) as char),
        0x1C => "Ctrl-\\".to_owned(),
        _ => format!("0x{byte:02X}"),
    }
}

/// Return the appropriate hint spans for the main view (no dialog open).
///
/// When `prefix_awaiting` is true the operator has pressed the prefix key and
/// a keymap-derived cheat-sheet of prefix commands replaces the normal
/// navigation hints so discovery is possible without a manual.
///
/// `palette_key` is the resolved palette-key byte (`self.palette_key.unwrap_or(0x1C)`
/// from [`crate::tui::input::InputParser`]); it drives the dynamic glyph so the
/// hint bar stays correct when `JACKIN_PALETTE_KEY` overrides the default.
pub(crate) fn main_view_hint(
    scrollback_active: bool,
    palette_key: u8,
    axes: termrock::scroll::ScrollAxes,
    prefix_awaiting: bool,
) -> Vec<HintSpan<'static>> {
    if prefix_awaiting {
        let mut spans = PREFIX_COMMAND_KEYMAP.hint_spans(); // all Shown prefix keys
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::DynKey(format_key_glyph(palette_key))); // dynamic palette key glyph
        spans.push(HintSpan::Text("palette"));
        spans.push(HintSpan::GroupSep);
        spans.extend(CAPSULE_GLOBAL_KEYMAP.hint_spans()); // Ctrl-Q quit
        return spans;
    }
    if scrollback_active {
        let mut spans = termrock::scroll::scroll_hint_spans(axes);
        if !spans.is_empty() {
            spans.push(HintSpan::GroupSep);
        }
        // UNREGISTERABLE(scrollback-modal): Esc handled by InputParser scrollback state check; no scrollback keymap exists.
        spans.push(HintSpan::Key("Esc"));
        spans.push(HintSpan::Text("exit scrollback"));
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::DynKey(format_key_glyph(palette_key)));
        spans.push(HintSpan::Text("menu"));
        spans.push(HintSpan::GroupSep);
        spans.extend(CAPSULE_GLOBAL_KEYMAP.hint_spans());
        spans
    } else {
        let mut spans = vec![
            HintSpan::DynKey(format_key_glyph(palette_key)),
            HintSpan::Text("menu"),
        ];
        let scroll = termrock::scroll::scroll_hint_spans(axes);
        if !scroll.is_empty() {
            spans.push(HintSpan::GroupSep);
            spans.extend(scroll);
        }
        spans.push(HintSpan::GroupSep);
        spans.extend(RESIZE_PANE_KEYMAP.hint_spans());
        spans.push(HintSpan::GroupSep);
        // UNREGISTERABLE(mouse): mouse click cannot be expressed as a KeyChord.
        spans.push(HintSpan::Key("click"));
        spans.push(HintSpan::Text("focus pane"));
        spans.push(HintSpan::GroupSep);
        spans.extend(CAPSULE_GLOBAL_KEYMAP.hint_spans());
        spans
    }
}

/// Shared footer for the filterable list dialogs. Every key glyph derives from
/// [`FILTER_LIST_KEYMAP`]; the call site supplies the contextual confirm label
/// (`"select"` vs `"launch"`) and whether the "type filter" group appears
/// (`ProviderPicker` has no filter input). Navigate keeps the keymap's own
/// `"navigate"` label; cancel keeps its `"cancel"` label.
fn filter_list_hint(confirm_label: &'static str, type_filter: bool) -> Vec<HintSpan<'static>> {
    let mut spans = Vec::with_capacity(10);
    FILTER_LIST_KEYMAP.push_spans_for(FilterListAction::NavigateUp, &mut spans);
    if type_filter {
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Text("type filter"));
    }
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Key(
        FILTER_LIST_KEYMAP.glyph_for(FilterListAction::Confirm),
    ));
    spans.push(HintSpan::Text(confirm_label));
    spans.push(HintSpan::GroupSep);
    FILTER_LIST_KEYMAP.push_spans_for(FilterListAction::Dismiss, &mut spans);
    spans
}

pub(super) fn palette_hint() -> Vec<HintSpan<'static>> {
    filter_list_hint("select", true)
}

pub(super) fn picker_hint() -> Vec<HintSpan<'static>> {
    filter_list_hint("launch", true)
}

/// Provider picker has no filter input — hint without the "type filter" group.
pub(super) fn provider_hint() -> Vec<HintSpan<'static>> {
    filter_list_hint("select", false)
}

/// Shared single-line text-input hint (commit + dismiss), plus an optional
/// `trailing` affordance span. Tab rename passes "empty = auto name"; file export
/// passes `None` because an empty export path is rejected, never auto-named.
fn text_input_hint(trailing: Option<&'static str>) -> Vec<HintSpan<'static>> {
    let mut spans = Vec::with_capacity(if trailing.is_some() { 7 } else { 5 });
    RENAME_KEYMAP.push_spans_for(RenameAction::Save, &mut spans);
    spans.push(HintSpan::GroupSep);
    RENAME_KEYMAP.push_spans_for(RenameAction::Dismiss, &mut spans);
    if let Some(text) = trailing {
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Text(text));
    }
    spans
}

pub(super) fn rename_hint() -> Vec<HintSpan<'static>> {
    text_input_hint(Some("empty = auto name"))
}

pub(super) fn export_file_hint() -> Vec<HintSpan<'static>> {
    text_input_hint(None)
}

/// Read-only info-dialog hint: copy key, the *available* scroll axes (per
/// `axes`, omitted when the body fits), then dismiss — built from the shared
/// `scroll_hint_spans` primitive so it never advertises a scroll direction the
/// body cannot move. Used by both `ContainerInfo` (Debug info) and a loaded
/// `GitHubContext`, which differ only in their copy label.
pub(super) fn info_dialog_hint(
    copy_label: &'static str,
    axes: termrock::scroll::ScrollAxes,
) -> Vec<HintSpan<'static>> {
    // UNREGISTERABLE(info-dialog-copy): Enter selects the active copy target inline; no InfoDialog keymap registered.
    let mut spans = vec![HintSpan::Key("↵"), HintSpan::Text(copy_label)];
    let scroll = termrock::scroll::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.push(HintSpan::GroupSep);
        spans.extend(scroll);
    }
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Key(
        READ_ONLY_DISMISS_KEYMAP.glyph_for(ReadOnlyDismissAction::Dismiss),
    ));
    spans.push(HintSpan::Text("dismiss"));
    spans
}

pub(super) fn usage_hint(axes: termrock::scroll::ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut spans = vec![
        HintSpan::Key(glyph::LEFT_RIGHT),
        HintSpan::Text("switch provider"),
        HintSpan::GroupSep,
        HintSpan::Key(glyph::TAB),
        HintSpan::Text("focus content"),
        HintSpan::GroupSep,
        HintSpan::Key("r"),
        HintSpan::Text("refresh"),
    ];
    let scroll = termrock::scroll::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.push(HintSpan::GroupSep);
        spans.extend(scroll);
    }
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Key(
        READ_ONLY_DISMISS_KEYMAP.glyph_for(ReadOnlyDismissAction::Dismiss),
    ));
    spans.push(HintSpan::Text("close"));
    spans
}

pub(super) fn read_only_hint() -> Vec<HintSpan<'static>> {
    READ_ONLY_DISMISS_KEYMAP.hint_spans()
}

pub(super) fn confirm_hint() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵"),
        HintSpan::Text("confirm"),
        HintSpan::GroupSep,
        HintSpan::Key("Y"),
        HintSpan::Text("yes"),
        HintSpan::GroupSep,
        HintSpan::Key("N/Esc"),
        HintSpan::Text("no"),
        HintSpan::GroupSep,
        HintSpan::Key("⇥"),
        HintSpan::Text("focus"),
    ]
}

#[cfg(test)]
mod tests;
