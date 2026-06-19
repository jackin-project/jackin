use super::{PREVIEW_PANE_KEYMAP, PreviewPaneAction, preview_pane_hint_spans, yes_no_hint_spans};
use jackin_tui::components::{KeyChord, LogicalKey};

#[test]
fn preview_pane_keymap_dispatches_all_nav_keys() {
    use PreviewPaneAction::*;
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(NavigatePane)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(NavigatePane)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('j'))),
        Some(NavigatePane)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('k'))),
        Some(NavigatePane)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Enter)),
        Some(Attach)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Esc)),
        Some(Back)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Left)),
        Some(Back)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::ctrl(LogicalKey::Char('q'))),
        Some(Quit)
    );
}

#[test]
fn preview_pane_hint_spans_contain_expected_keys() {
    let spans = preview_pane_hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("↑↓"), "must advertise up/down: {text}");
    assert!(text.contains("navigate panes"), "must label nav: {text}");
    assert!(text.contains("↵"), "must advertise enter: {text}");
    assert!(text.contains("Esc"), "must advertise esc back: {text}");
    // Ctrl-Q is Internal in PREVIEW_PANE_KEYMAP — handled upstream by
    // should_open_quit_confirm before reaching handle_preview_focused_key.
    assert!(!text.contains("Ctrl-Q"), "quit must NOT appear in preview hint (handled upstream): {text}");
}

#[test]
fn yes_no_hint_spans_include_enter_confirm() {
    let spans = yes_no_hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("confirm") || text.contains("yes"), "must have yes: {text}");
}
