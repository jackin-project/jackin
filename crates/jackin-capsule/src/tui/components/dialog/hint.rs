//! Footer hint rows for capsule dialogs.

use jackin_tui::HintSpan;

use crate::tui::keymap::{
    FILTER_LIST_KEYMAP, FilterListAction, READ_ONLY_DISMISS_KEYMAP, RENAME_KEYMAP, RenameAction,
};

/// Return the appropriate hint spans for the main view (no dialog open).
///
/// When `prefix_awaiting` is true the operator has pressed the prefix key and
/// a compact cheat-sheet of the most-used prefix commands replaces the normal
/// navigation hints so discovery is possible without a manual.
pub(crate) fn main_view_hint(
    scrollback_active: bool,
    axes: jackin_tui::components::ScrollAxes,
    prefix_awaiting: bool,
) -> Vec<HintSpan<'static>> {
    if prefix_awaiting {
        return vec![
            HintSpan::Key("space/:"),
            HintSpan::Text("palette"),
            HintSpan::GroupSep,
            HintSpan::Key("h/j/k/l"),
            HintSpan::Text("split/nav"),
            HintSpan::GroupSep,
            HintSpan::Key("n/c"),
            HintSpan::Text("new/close"),
            HintSpan::GroupSep,
            HintSpan::Key("Ctrl-Q"),
            HintSpan::Text("quit"),
        ];
    }
    if scrollback_active {
        let mut spans = jackin_tui::components::scroll_hint_spans(axes);
        if !spans.is_empty() {
            spans.push(HintSpan::GroupSep);
        }
        spans.push(HintSpan::Key("Esc"));
        spans.push(HintSpan::Text("exit scrollback"));
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Key("Ctrl+\\"));
        spans.push(HintSpan::Text("menu"));
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Key("Ctrl-Q"));
        spans.push(HintSpan::Text("quit"));
        spans
    } else {
        let mut spans = vec![HintSpan::Key("Ctrl+\\"), HintSpan::Text("menu")];
        let scroll = jackin_tui::components::scroll_hint_spans(axes);
        if !scroll.is_empty() {
            spans.push(HintSpan::GroupSep);
            spans.extend(scroll);
        }
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Key("Alt+Shift+↑↓←→"));
        spans.push(HintSpan::Text("resize pane"));
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Key("click"));
        spans.push(HintSpan::Text("focus pane"));
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Key("Ctrl-Q"));
        spans.push(HintSpan::Text("quit"));
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

pub(super) fn rename_hint() -> Vec<HintSpan<'static>> {
    let mut spans = Vec::with_capacity(7);
    RENAME_KEYMAP.push_spans_for(RenameAction::Save, &mut spans);
    spans.push(HintSpan::GroupSep);
    RENAME_KEYMAP.push_spans_for(RenameAction::Dismiss, &mut spans);
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Text("empty = auto name"));
    spans
}

/// Read-only info-dialog hint: copy key, the *available* scroll axes (per
/// `axes`, omitted when the body fits), then dismiss — built from the shared
/// `scroll_hint_spans` primitive so it never advertises a scroll direction the
/// body cannot move. Used by both `ContainerInfo` (Debug info) and a loaded
/// `GitHubContext`, which differ only in their copy label.
pub(super) fn info_dialog_hint(
    copy_label: &'static str,
    axes: jackin_tui::components::ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let mut spans = vec![HintSpan::Key("↵"), HintSpan::Text(copy_label)];
    let scroll = jackin_tui::components::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.push(HintSpan::GroupSep);
        spans.extend(scroll);
    }
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Key("Esc"));
    spans.push(HintSpan::Text("dismiss"));
    spans
}

pub(super) fn read_only_hint() -> Vec<HintSpan<'static>> {
    READ_ONLY_DISMISS_KEYMAP.hint_spans()
}

pub(super) fn confirm_hint() -> Vec<HintSpan<'static>> {
    jackin_tui::components::confirm_hint_spans()
}

#[cfg(test)]
mod tests;
