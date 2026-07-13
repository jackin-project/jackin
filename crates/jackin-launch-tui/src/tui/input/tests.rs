// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crossterm::event::KeyEvent;

#[test]
fn second_ctrl_c_inside_window_hard_exits() {
    let mut detector = DoubleCtrlC::new(Duration::from_millis(750));
    let start = Instant::now();
    let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(detector.observe(&ctrl_c, start), CtrlCAction::Continue);
    assert_eq!(
        detector.observe(&ctrl_c, start + Duration::from_millis(100)),
        CtrlCAction::HardExit
    );
}

#[test]
fn ctrl_c_outside_window_starts_new_sequence() {
    let mut detector = DoubleCtrlC::new(Duration::from_millis(750));
    let start = Instant::now();
    let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(detector.observe(&ctrl_c, start), CtrlCAction::Continue);
    assert_eq!(
        detector.observe(&ctrl_c, start + Duration::from_millis(900)),
        CtrlCAction::Continue
    );
}

#[test]
fn non_ctrl_c_resets_sequence() {
    let mut detector = DoubleCtrlC::new(Duration::from_millis(750));
    let start = Instant::now();
    let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let other = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

    assert_eq!(detector.observe(&ctrl_c, start), CtrlCAction::Continue);
    assert_eq!(
        detector.observe(&other, start + Duration::from_millis(100)),
        CtrlCAction::Continue
    );
    assert_eq!(
        detector.observe(&ctrl_c, start + Duration::from_millis(200)),
        CtrlCAction::Continue
    );
}

#[test]
fn forced_terminal_restore_disables_mouse_reporting() {
    let mut out = Vec::new();

    write_forced_terminal_restore(&mut out).expect("restore bytes");
    let rendered = String::from_utf8(out).expect("utf8 escape bytes");

    assert!(rendered.contains("\x1b[?1006l"));
    assert!(rendered.contains("\x1b[?1003l"));
    assert!(rendered.contains("\x1b[?1002l"));
    assert!(rendered.contains("\x1b[?1000l"));
}

#[test]
fn forced_terminal_restore_resets_other_host_modes() {
    let mut out = Vec::new();

    write_forced_terminal_restore(&mut out).expect("restore bytes");
    let rendered = String::from_utf8(out).expect("utf8 escape bytes");

    assert!(rendered.contains(jackin_tui::ansi::RESET));
    assert!(rendered.contains(jackin_tui::ansi::POINTER_DEFAULT));
    assert!(rendered.contains("\x1b[?1004l"));
    assert!(rendered.contains("\x1b[?2004l"));
    assert!(rendered.contains("\x1b[?25h"));
}
