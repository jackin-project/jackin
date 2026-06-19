use super::PREFIX_COMMAND_KEYMAP;
use crate::tui::input::{ArrowDir, PrefixCommand};
use jackin_tui::components::{KeyChord, LogicalKey};
use jackin_tui::keymap::raw_bytes_to_chord;

#[test]
fn prefix_keymap_covers_all_prefix_binding_keys() {
    let cases: &[(&[u8], PrefixCommand)] = &[
        (b"c", PrefixCommand::NewTab),
        (b"n", PrefixCommand::NextTab),
        (b"p", PrefixCommand::PrevTab),
        (b"0", PrefixCommand::JumpTab(0)),
        (b"9", PrefixCommand::JumpTab(9)),
        (b"h", PrefixCommand::MoveFocus(ArrowDir::Left)),
        (b"j", PrefixCommand::MoveFocus(ArrowDir::Down)),
        (b"k", PrefixCommand::MoveFocus(ArrowDir::Up)),
        (b"l", PrefixCommand::MoveFocus(ArrowDir::Right)),
        (b"z", PrefixCommand::ZoomToggle),
        (b"x", PrefixCommand::KillPane),
        (b"&", PrefixCommand::KillTab),
        (b"d", PrefixCommand::Detach),
        (b" ", PrefixCommand::Palette),
        (b":", PrefixCommand::Palette),
        (b"r", PrefixCommand::Redraw),
    ];
    for (raw, expected) in cases {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        let action = PREFIX_COMMAND_KEYMAP.dispatch(chord)
            .unwrap_or_else(|| panic!("keymap did not dispatch {raw:?} (chord {chord:?})"));
        assert_eq!(action, *expected, "wrong action for {raw:?}");
    }
}

#[test]
fn prefix_keymap_covers_ctrl_l() {
    let chord = raw_bytes_to_chord(&[0x0c]).expect("0x0c → Ctrl-L");
    assert_eq!(chord, KeyChord::ctrl(LogicalKey::Char('l')));
    assert_eq!(
        PREFIX_COMMAND_KEYMAP.dispatch(chord),
        Some(PrefixCommand::ClearPane),
        "Ctrl-L must dispatch to ClearPane"
    );
}

#[test]
fn prefix_keymap_has_shown_hints_for_primary_commands() {
    let spans = PREFIX_COMMAND_KEYMAP.hint_spans();
    let keys: Vec<&str> = spans.iter().filter_map(|s| match s {
        jackin_tui::HintSpan::Key(k) => Some(*k),
        _ => None,
    }).collect();
    // Primary navigation commands must be advertised
    assert!(keys.contains(&"h"), "must show h: {keys:?}");
    assert!(keys.contains(&"j"), "must show j: {keys:?}");
    assert!(keys.contains(&"k"), "must show k: {keys:?}");
    assert!(keys.contains(&"l"), "must show l: {keys:?}");
    assert!(keys.contains(&"c"), "must show c: {keys:?}");
    assert!(keys.contains(&"x"), "must show x: {keys:?}");
}
