//! Tests for `build_log`.
use super::*;

// One test only: the buffer is a process-global static, so splitting these
// assertions into parallel tests would race on the shared state.
#[test]
fn buffer_caps_and_clears() {
    let _guard = TEST_LOCK.lock().unwrap();
    begin();
    for i in 0..(MAX_LINES + 10) {
        push_line(&format!("line {i}"));
    }
    assert_eq!(len(), MAX_LINES);
    let snap = snapshot();
    assert_eq!(snap.first().map(String::as_str), Some("line 10"));
    assert_eq!(
        snap.last().map(String::as_str),
        Some(&*format!("line {}", MAX_LINES + 9))
    );

    // begin() resets the buffer.
    begin();
    push_line("only");
    assert_eq!(snapshot(), vec!["only"]);

    end();
    assert!(!is_active());
}
