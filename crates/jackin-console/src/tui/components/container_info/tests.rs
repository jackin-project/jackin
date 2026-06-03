//! Tests for `container_info`.
use super::debug_run_info_state;

#[test]
fn debug_run_info_state_marks_run_id_copyable_and_log_hyperlinked() {
    let state = debug_run_info_state("run-1", "/tmp/jackin/run.log");
    let rows = state.rows();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].value(), "run-1");
    assert!(rows[0].is_copyable());
    assert_eq!(rows[1].value(), "/tmp/jackin/run.log");
    assert_eq!(rows[1].href(), Some("file:///tmp/jackin/run.log"));
}
