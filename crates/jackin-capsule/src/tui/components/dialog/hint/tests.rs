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
