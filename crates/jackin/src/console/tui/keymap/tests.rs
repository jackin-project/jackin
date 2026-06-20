use super::{
    EDITOR_TOP_LEVEL_KEYMAP, PREVIEW_PANE_KEYMAP, WORKSPACE_LIST_KEYMAP, EditorTopLevelAction,
    PreviewPaneAction, WorkspaceListAction, preview_pane_hint_spans, yes_no_hint_spans,
};
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

// ── WORKSPACE_LIST_KEYMAP ─────────────────────────────────────────────────────

#[test]
fn workspace_list_keymap_dispatches_arrow_nav() {
    assert_eq!(
        WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(WorkspaceListAction::Navigate)
    );
    assert_eq!(
        WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(WorkspaceListAction::Navigate)
    );
}

#[test]
fn workspace_list_keymap_vim_nav_aliases() {
    for ch in ['j', 'J', 'k', 'K'] {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(WorkspaceListAction::Navigate),
            "vim alias '{ch}' must navigate"
        );
    }
}

#[test]
fn workspace_list_keymap_vim_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(WorkspaceListAction::Left),
            "vim alias '{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(WorkspaceListAction::Right),
            "vim alias '{ch}' must scroll right"
        );
    }
}

#[test]
fn workspace_list_keymap_action_keys() {
    use WorkspaceListAction::*;
    let cases: &[(LogicalKey, WorkspaceListAction)] = &[
        (LogicalKey::Enter, Enter),
        (LogicalKey::Char('e'), Edit),
        (LogicalKey::Char('E'), Edit),
        (LogicalKey::Char('n'), NewSession),
        (LogicalKey::Char('N'), NewSession),
        (LogicalKey::Char('d'), Delete),
        (LogicalKey::Char('D'), Delete),
        (LogicalKey::Char('s'), Settings),
        (LogicalKey::Char('S'), Settings),
        (LogicalKey::Char('o'), OpenGithub),
        (LogicalKey::Char('O'), OpenGithub),
        (LogicalKey::Tab, EnterPreview),
        (LogicalKey::Esc, Exit),
        (LogicalKey::Char('q'), Exit),
        (LogicalKey::Char('Q'), Exit),
    ];
    for (key, expected) in cases {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(*key)),
            Some(*expected),
            "key {key:?} must map to {expected:?}"
        );
    }
    assert_eq!(
        WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::ctrl(LogicalKey::Char('q'))),
        Some(Quit)
    );
}

// ── EDITOR_TOP_LEVEL_KEYMAP ───────────────────────────────────────────────────

#[test]
fn editor_keymap_dispatches_arrow_nav() {
    assert_eq!(
        EDITOR_TOP_LEVEL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(EditorTopLevelAction::MoveField)
    );
    assert_eq!(
        EDITOR_TOP_LEVEL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(EditorTopLevelAction::MoveField)
    );
}

#[test]
fn editor_keymap_vim_nav_aliases() {
    for ch in ['j', 'J', 'k', 'K'] {
        assert_eq!(
            EDITOR_TOP_LEVEL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorTopLevelAction::MoveField),
            "vim alias '{ch}' must move field"
        );
    }
}

#[test]
fn editor_keymap_vim_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            EDITOR_TOP_LEVEL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorTopLevelAction::ScrollLeft),
            "vim alias '{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            EDITOR_TOP_LEVEL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorTopLevelAction::ScrollRight),
            "vim alias '{ch}' must scroll right"
        );
    }
}

#[test]
fn editor_keymap_action_keys() {
    use EditorTopLevelAction::*;
    let cases: &[(LogicalKey, EditorTopLevelAction)] = &[
        (LogicalKey::Tab, NextTab),
        (LogicalKey::BackTab, FocusTabBar),
        (LogicalKey::Char('s'), Save),
        (LogicalKey::Char('S'), Save),
        (LogicalKey::Esc, Escape),
    ];
    for (key, expected) in cases {
        assert_eq!(
            EDITOR_TOP_LEVEL_KEYMAP.dispatch(KeyChord::plain(*key)),
            Some(*expected),
            "key {key:?} must map to {expected:?}"
        );
    }
    assert_eq!(
        EDITOR_TOP_LEVEL_KEYMAP.dispatch(KeyChord::ctrl(LogicalKey::Char('q'))),
        Some(Quit)
    );
}
