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
    HintSpan::Key("Esc"),
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
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
];

pub(super) const RENAME_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
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
    &[HintSpan::Key("Esc"), HintSpan::Text("dismiss")];

pub(super) const CONFIRM_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Y"),
    HintSpan::Text("confirm"),
    HintSpan::GroupSep,
    HintSpan::Key("N"),
    HintSpan::Text("cancel"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("back"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(spans: &[HintSpan<'_>]) -> String {
        spans
            .iter()
            .filter_map(|span| match span {
                HintSpan::Key(text) | HintSpan::Text(text) => Some((*text).to_owned()),
                HintSpan::Dyn(text) => Some(text.clone()),
                HintSpan::Sep | HintSpan::GroupSep => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn main_view_hint_omits_scroll_when_focused_pane_fits() {
        let hint = labels(&main_view_hint(
            false,
            jackin_tui::components::ScrollAxes::default(),
        ));
        assert!(hint.contains("Ctrl+\\ menu"));
        assert!(hint.contains("click focus pane"));
        assert!(
            !hint.contains("scroll"),
            "fit-content main view must not advertise scroll: {hint}"
        );
    }

    #[test]
    fn main_view_hint_advertises_only_visible_scroll_axis() {
        let hint = labels(&main_view_hint(
            false,
            jackin_tui::components::ScrollAxes {
                vertical: true,
                horizontal: false,
            },
        ));
        assert!(hint.contains("↑↓ scroll"));
        assert!(
            !hint.contains("←→"),
            "vertical-only pane must not advertise horizontal scroll: {hint}"
        );
    }

    #[test]
    fn scrollback_hint_omits_scroll_when_no_axis_is_visible() {
        let hint = labels(&main_view_hint(
            true,
            jackin_tui::components::ScrollAxes::default(),
        ));
        assert!(hint.contains("Esc exit scrollback"));
        assert!(hint.contains("Ctrl+\\ menu"));
        assert!(
            !hint.contains("↑↓ scroll"),
            "scrollback exit hint must not advertise scroll without a visible axis: {hint}"
        );
    }
}
