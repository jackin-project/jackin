use super::*;
use crate::geometry::HintSpan;
use crate::scroll::ScrollAxes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestAction {
    Confirm,
    Cancel,
    Navigate,
    HiddenVim,
}

const TEST_BINDINGS: &[KeyBinding<TestAction>] = &[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: TestAction::Confirm,
        hint: Some("confirm"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Esc),
            KeyChord::plain(LogicalKey::Char('n')),
            KeyChord::plain(LogicalKey::Char('N')),
        ],
        action: TestAction::Cancel,
        hint: Some("cancel"),
        visibility: Visibility::Shown,
        glyph: Some("N/Esc"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Up), KeyChord::plain(LogicalKey::Down)],
        action: TestAction::Navigate,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("\u{2191}\u{2193}"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('k')), KeyChord::plain(LogicalKey::Char('j'))],
        action: TestAction::HiddenVim,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
];

static TEST_MAP: Keymap<TestAction> = Keymap::new(TEST_BINDINGS);

#[test]
fn dispatch_finds_primary_chord() {
    assert_eq!(TEST_MAP.dispatch(KeyChord::plain(LogicalKey::Enter)), Some(TestAction::Confirm));
}

#[test]
fn dispatch_finds_alias_chord() {
    assert_eq!(TEST_MAP.dispatch(KeyChord::plain(LogicalKey::Esc)), Some(TestAction::Cancel));
    assert_eq!(
        TEST_MAP.dispatch(KeyChord::plain(LogicalKey::Char('n'))),
        Some(TestAction::Cancel)
    );
    assert_eq!(
        TEST_MAP.dispatch(KeyChord::plain(LogicalKey::Char('k'))),
        Some(TestAction::HiddenVim)
    );
}

#[test]
fn dispatch_returns_none_for_unbound_chord() {
    assert_eq!(TEST_MAP.dispatch(KeyChord::plain(LogicalKey::Tab)), None);
}

#[test]
fn hint_spans_only_includes_shown_bindings() {
    let spans = TEST_MAP.hint_spans();
    let keys: Vec<&str> = spans
        .iter()
        .filter_map(|s| if let HintSpan::Key(k) = s { Some(*k) } else { None })
        .collect();
    assert!(keys.contains(&"\u{21b5}"), "should have Enter glyph (↵)");
    assert!(keys.contains(&"N/Esc"), "should have glyph override");
    assert!(keys.contains(&"\u{2191}\u{2193}"), "should have grouped arrow glyph (↑↓)");
    // HiddenAlias should be absent
    assert!(!keys.contains(&"K"), "vim alias should not appear");
}

#[test]
fn hint_spans_for_axes_omits_arrows_when_no_scroll() {
    let axes = ScrollAxes { vertical: false, horizontal: false };
    let spans = TEST_MAP.hint_spans_for_axes(axes);
    let keys: Vec<&str> = spans
        .iter()
        .filter_map(|s| if let HintSpan::Key(k) = s { Some(*k) } else { None })
        .collect();
    assert!(!keys.contains(&"\u{2191}\u{2193}"), "arrow group must be omitted when no scroll");
    assert!(keys.contains(&"\u{21b5}"), "Enter must still be shown");
}

#[test]
fn hint_spans_for_axes_includes_arrows_when_vertical_available() {
    let axes = ScrollAxes { vertical: true, horizontal: false };
    let spans = TEST_MAP.hint_spans_for_axes(axes);
    let keys: Vec<&str> = spans
        .iter()
        .filter_map(|s| if let HintSpan::Key(k) = s { Some(*k) } else { None })
        .collect();
    assert!(keys.contains(&"\u{2191}\u{2193}"), "arrow group must show when vertical available");
}

#[test]
fn chord_glyph_reproduces_existing_glyphs() {
    assert_eq!(chord_glyph(Some(KeyChord::ctrl(LogicalKey::Char('q')))), "Ctrl+Q");
    assert_eq!(chord_glyph(Some(KeyChord::ctrl(LogicalKey::Char('c')))), "Ctrl-C");
    assert_eq!(chord_glyph(Some(KeyChord::plain(LogicalKey::Enter))), "\u{21b5}");
    assert_eq!(chord_glyph(Some(KeyChord::plain(LogicalKey::Esc))), "Esc");
    assert_eq!(chord_glyph(Some(KeyChord::plain(LogicalKey::Tab))), "\u{21e5}");
    assert_eq!(chord_glyph(Some(KeyChord::plain(LogicalKey::Up))), "\u{2191}");
    assert_eq!(chord_glyph(Some(KeyChord::plain(LogicalKey::Down))), "\u{2193}");
    assert_eq!(chord_glyph(Some(KeyChord::plain(LogicalKey::Char('y')))), "Y");
    assert_eq!(chord_glyph(Some(KeyChord::plain(LogicalKey::Char('Y')))), "Y");
    assert_eq!(chord_glyph(None), "");
}

#[test]
fn mods_bit_flags_combine_correctly() {
    let ctrl_shift = Mods::NONE.with_ctrl().with_shift();
    assert!(ctrl_shift.contains(Mods::CTRL));
    assert!(ctrl_shift.contains(Mods::SHIFT));
    assert!(!ctrl_shift.contains(Mods::ALT));
    assert!(!ctrl_shift.is_empty());
    assert!(Mods::NONE.is_empty());
}

#[test]
fn from_crossterm_key_event_converts_basic_keys() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let ev = KeyEvent {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let chord = KeyChord::from(ev);
    assert_eq!(chord.key, LogicalKey::Char('q'));
    assert!(chord.mods.contains(Mods::CTRL));

    let ev2 = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    assert_eq!(KeyChord::from(ev2), KeyChord::plain(LogicalKey::Enter));
}
