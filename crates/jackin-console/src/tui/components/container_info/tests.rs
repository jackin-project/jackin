//! Tests for `container_info`.
use super::debug_run_info_state;

#[test]
fn debug_run_info_state_marks_run_id_copyable_and_log_hyperlinked() {
    let state = debug_run_info_state("0.6.0-test", "run-1", "/tmp/jackin/run.log");
    let rows = state.rows();

    // Canonical order with only run fields known: jackin, Run ID, Diagnostics log.
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].value(), "0.6.0-test");
    assert_eq!(rows[1].value(), "run-1", "Run ID must be the bare id, not the log path");
    assert!(rows[1].is_copyable());
    assert_eq!(rows[2].value(), "/tmp/jackin/run.log");
    assert_eq!(rows[2].href(), Some("file:///tmp/jackin/run.log"));
    assert!(rows[2].is_copyable(), "diagnostics log path must be copyable");
}
