//! Footer hint rows for capsule dialogs.

use jackin_tui::HintSpan;

/// Return the appropriate hint spans for the main view (no dialog open).
pub(crate) fn main_view_hint(
    scrollback_active: bool,
    axes: jackin_tui::components::ScrollAxes,
) -> Vec<HintSpan<'static>> {
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
        spans.push(HintSpan::Key("click"));
        spans.push(HintSpan::Text("focus pane"));
        spans.push(HintSpan::GroupSep);
        spans.push(HintSpan::Key("Ctrl-Q"));
        spans.push(HintSpan::Text("quit"));
        spans
    }
}

pub(super) const PALETTE_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type filter"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
    HintSpan::Text("select"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl-C/Esc"),
    HintSpan::Text("cancel"),
];

pub(super) const PICKER_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type filter"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
    HintSpan::Text("launch"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl-C/Esc"),
    HintSpan::Text("cancel"),
];

/// Provider picker has no filter input — dedicated hint without "type filter".
pub(super) const PROVIDER_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
    HintSpan::Text("select"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl-C/Esc"),
    HintSpan::Text("cancel"),
];

pub(super) const RENAME_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl-C/Esc"),
    HintSpan::Text("cancel"),
    HintSpan::GroupSep,
    HintSpan::Text("empty = auto name"),
];

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

pub(super) const READ_ONLY_HINT: &[HintSpan<'static>] =
    &[HintSpan::Key("q/Esc"), HintSpan::Text("dismiss")];

pub(super) fn confirm_hint() -> Vec<HintSpan<'static>> {
    jackin_tui::components::confirm_hint_spans()
}

#[cfg(test)]
mod tests;
