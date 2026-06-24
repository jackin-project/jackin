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
