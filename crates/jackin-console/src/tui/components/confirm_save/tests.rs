//! Tests for `confirm_save`.
use super::*;
use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};
use jackin_tui::{
    HintSpan,
    components::ButtonFocus,
    keymap::{KeyChord, LogicalKey},
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn sample_state() -> ConfirmSaveState {
    ConfirmSaveState::new(vec![Line::from("Create workspace: demo")])
}

#[test]
fn confirm_save_defaults_to_cancel_focus() {
    // Default = Cancel so Enter on a freshly-opened dialog never fires
    // the save arm (TUI design decisions: confirmation dialog rule).
    let s = sample_state();
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
}

#[test]
fn confirm_save_tab_cycles_cancel_save() {
    let mut s = sample_state();
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
    assert!(matches!(
        s.handle_key(key(KeyCode::Tab)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.focus, ConfirmSaveFocus::Save);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
}

#[test]
fn confirm_save_left_cycles_reverse() {
    let mut s = sample_state();
    // Starts at Cancel; Left toggles to Save.
    s.handle_key(key(KeyCode::Left));
    assert_eq!(s.focus, ConfirmSaveFocus::Save);
    s.handle_key(key(KeyCode::Left));
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
}

#[test]
fn confirm_save_focus_uses_button_focus_ring() {
    assert_eq!(ConfirmSaveFocus::Cancel.next(), ConfirmSaveFocus::Save);
    assert_eq!(ConfirmSaveFocus::Save.next(), ConfirmSaveFocus::Cancel);
    assert_eq!(ConfirmSaveFocus::Save.prev(), ConfirmSaveFocus::Cancel);
    assert_eq!(ConfirmSaveFocus::Cancel.prev(), ConfirmSaveFocus::Save);
}

#[test]
fn confirm_save_enter_on_cancel_returns_cancel() {
    // Default focus = Cancel, so Enter fires Cancel immediately.
    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn confirm_save_enter_on_save_commits_save_choice() {
    // Tab once (Cancel -> Save) then Enter commits Save.
    let mut s = sample_state();
    s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(SaveChoice::Save)
    ));
}

#[test]
fn confirm_save_s_shortcut_commits_save() {
    let mut s = sample_state();
    // Rotate focus first to prove the shortcut is focus-independent.
    s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('s'))),
        ModalOutcome::Commit(SaveChoice::Save)
    ));

    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('S'))),
        ModalOutcome::Commit(SaveChoice::Save)
    ));
}

#[test]
fn confirm_save_c_shortcut_cancels() {
    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('c'))),
        ModalOutcome::Cancel
    ));

    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('C'))),
        ModalOutcome::Cancel
    ));
}

#[test]
fn confirm_save_esc_cancels() {
    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn confirm_save_keymap_dispatches_legacy_bindings() {
    use ConfirmSaveAction::{Activate, Cancel, FocusNext, FocusPrev, Save, ScrollDown, ScrollUp};

    let cases = [
        (KeyCode::Char('s'), Save),
        (KeyCode::Char('S'), Save),
        (KeyCode::Char('c'), Cancel),
        (KeyCode::Char('C'), Cancel),
        (KeyCode::Esc, Cancel),
        (KeyCode::Enter, Activate),
        (KeyCode::Tab, FocusNext),
        (KeyCode::Right, FocusNext),
        (KeyCode::Char('l'), FocusNext),
        (KeyCode::Char('L'), FocusNext),
        (KeyCode::BackTab, FocusPrev),
        (KeyCode::Left, FocusPrev),
        (KeyCode::Char('h'), FocusPrev),
        (KeyCode::Char('H'), FocusPrev),
        (KeyCode::Up, ScrollUp),
        (KeyCode::Char('k'), ScrollUp),
        (KeyCode::Char('K'), ScrollUp),
        (KeyCode::Down, ScrollDown),
        (KeyCode::Char('j'), ScrollDown),
        (KeyCode::Char('J'), ScrollDown),
    ];

    for (code, expected) in cases {
        assert_eq!(
            CONFIRM_SAVE_KEYMAP.dispatch(KeyChord::from(key(code))),
            Some(expected),
            "legacy binding {code:?} dispatches"
        );
    }
}

#[test]
fn confirm_save_keymap_covers_each_action() {
    use ConfirmSaveAction::{Activate, Cancel, FocusNext, FocusPrev, Save, ScrollDown, ScrollUp};

    for expected in [
        Activate, Save, Cancel, FocusNext, FocusPrev, ScrollUp, ScrollDown,
    ] {
        assert!(
            CONFIRM_SAVE_KEYMAP
                .bindings()
                .iter()
                .any(|binding| binding.action == expected),
            "missing {expected:?} binding"
        );
    }
}

#[test]
fn confirm_save_keymap_does_not_bind_unknown_keys() {
    assert_eq!(
        CONFIRM_SAVE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('x'))),
        None
    );
}

#[test]
fn required_height_accounts_for_chrome() {
    let s = ConfirmSaveState::<()>::new(vec![
        Line::from("one"),
        Line::from("two"),
        Line::from("three"),
    ]);
    // 3 content lines + 6 chrome rows (2 borders + leading + spacer + buttons + trailing)
    assert_eq!(required_height(&s), 9);
}

#[test]
fn confirm_save_scroll_keys_start_from_clamped_offset() {
    let mut s = ConfirmSaveState::<()>::new(vec![
        Line::from("one"),
        Line::from("two"),
        Line::from("three"),
        Line::from("four"),
    ]);
    s.preview_rows = 2;
    s.scroll_offset = 99;

    s.handle_key(key(KeyCode::Down));
    assert_eq!(s.scroll_offset, 2);

    s.handle_key(key(KeyCode::Up));
    assert_eq!(s.scroll_offset, 1);
}

#[test]
fn confirm_save_hint_spans_derive_from_keymap_without_scroll() {
    let spans = confirm_save_hint_spans_for_axes(ScrollAxes::none());

    assert_eq!(
        spans,
        vec![
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("S"),
            HintSpan::Text("save"),
            HintSpan::GroupSep,
            HintSpan::Key("C/Esc"),
            HintSpan::Text("cancel"),
            HintSpan::GroupSep,
            HintSpan::Key("⇥/→"),
            HintSpan::Text("move"),
        ]
    );
}

#[test]
fn confirm_save_hint_spans_include_scroll_when_vertical_overflows() {
    let spans = confirm_save_hint_spans_for_axes(ScrollAxes {
        vertical: true,
        horizontal: false,
    });

    assert!(spans.contains(&HintSpan::Key("↑↓/j/k")));
    assert!(spans.contains(&HintSpan::Text("scroll")));
}
