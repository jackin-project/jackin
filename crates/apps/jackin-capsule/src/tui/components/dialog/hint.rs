// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use termrock::{keymap::glyph, widgets::HintSpan};

use crate::tui::keymap::{
    CAPSULE_GLOBAL_KEYMAP, FILTER_LIST_KEYMAP, FilterListAction, PREFIX_COMMAND_KEYMAP,
    READ_ONLY_DISMISS_KEYMAP, RENAME_KEYMAP, RESIZE_PANE_KEYMAP, ReadOnlyDismissAction,
    RenameAction,
};

fn key_span(glyph: impl Into<Cow<'static, str>>) -> HintSpan<'static> {
    match glyph.into() {
        Cow::Borrowed(glyph) => HintSpan::Key(glyph),
        Cow::Owned(glyph) => HintSpan::DynKey(glyph),
    }
}

fn format_key_glyph(byte: u8) -> String {
    match byte {
        0x01..=0x1A => format!("Ctrl-{}", (b'@' + byte) as char),
        0x1C => "Ctrl-\\".to_owned(),
        _ => format!("0x{byte:02X}"),
    }
}

pub(crate) fn main_view_hint(
    scrollback_active: bool,
    palette_key: u8,
    axes: termrock::scroll::ScrollAxes,
    prefix_awaiting: bool,
) -> Vec<HintSpan<'static>> {
    if prefix_awaiting {
        let mut spans = PREFIX_COMMAND_KEYMAP.hint_spans();
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::DynKey(format_key_glyph(palette_key)));
        spans.push(HintSpan::Text("palette"));
        spans.push(HintSpan::GroupSep);
        spans.extend(CAPSULE_GLOBAL_KEYMAP.hint_spans());
        return spans;
    }
    if scrollback_active {
        let mut spans = termrock::scroll::scroll_hint_spans(axes);
        if !spans.is_empty() {
            spans.push(HintSpan::GroupSep);
        }
        spans.push(key_span("Esc"));
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
        spans.push(key_span("click"));
        spans.push(HintSpan::Text("focus pane"));
        spans.push(HintSpan::GroupSep);
        spans.extend(CAPSULE_GLOBAL_KEYMAP.hint_spans());
        spans
    }
}

fn filter_list_hint(confirm_label: &'static str, type_filter: bool) -> Vec<HintSpan<'static>> {
    let mut spans = Vec::with_capacity(10);
    FILTER_LIST_KEYMAP.push_spans_for(FilterListAction::NavigateUp, &mut spans);
    if type_filter {
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Text("type filter"));
    }
    spans.push(HintSpan::GroupSep);
    spans.push(key_span(
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

pub(super) fn flat_picker_hint() -> Vec<HintSpan<'static>> {
    filter_list_hint("select", false)
}

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

pub(super) fn info_dialog_hint(
    copy_label: &'static str,
    axes: termrock::scroll::ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let mut spans = vec![key_span("↵"), HintSpan::Text(copy_label)];
    let scroll = termrock::scroll::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.push(HintSpan::GroupSep);
        spans.extend(scroll);
    }
    spans.push(HintSpan::GroupSep);
    spans.push(key_span(
        READ_ONLY_DISMISS_KEYMAP.glyph_for(ReadOnlyDismissAction::Dismiss),
    ));
    spans.push(HintSpan::Text("dismiss"));
    spans
}

pub(super) fn usage_hint(axes: termrock::scroll::ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut spans = vec![
        key_span(glyph::LEFT_RIGHT),
        HintSpan::Text("switch provider"),
        HintSpan::GroupSep,
        key_span(glyph::TAB),
        HintSpan::Text("focus content"),
        HintSpan::GroupSep,
        key_span("r"),
        HintSpan::Text("refresh"),
    ];
    let scroll = termrock::scroll::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.push(HintSpan::GroupSep);
        spans.extend(scroll);
    }
    spans.push(HintSpan::GroupSep);
    spans.push(key_span(
        READ_ONLY_DISMISS_KEYMAP.glyph_for(ReadOnlyDismissAction::Dismiss),
    ));
    spans.push(HintSpan::Text("close"));
    spans
}

pub(super) fn usage_empty_hint(axes: termrock::scroll::ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut spans = Vec::new();
    let scroll = termrock::scroll::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.extend(scroll);
        spans.push(HintSpan::GroupSep);
    }
    spans.extend([
        key_span(READ_ONLY_DISMISS_KEYMAP.glyph_for(ReadOnlyDismissAction::Dismiss)),
        HintSpan::Text("close"),
    ]);
    spans
}

pub(super) fn read_only_hint() -> Vec<HintSpan<'static>> {
    READ_ONLY_DISMISS_KEYMAP.hint_spans()
}

pub(super) fn confirm_hint() -> Vec<HintSpan<'static>> {
    vec![
        key_span("↵"),
        HintSpan::Text("confirm"),
        HintSpan::GroupSep,
        key_span("Y"),
        HintSpan::Text("yes"),
        HintSpan::GroupSep,
        key_span("N/Esc"),
        HintSpan::Text("no"),
        HintSpan::GroupSep,
        key_span("⇥"),
        HintSpan::Text("focus"),
    ]
}

#[cfg(test)]
mod tests;
