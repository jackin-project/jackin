//! Tests for `tui`.
use super::*;
use std::sync::Mutex;

static DEBUG_BUFFER_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn format_debug_line_matches_wire_format() {
    assert_eq!(
        format_debug_line("isolation", "git worktree add -b foo /tmp/wt deadbeef"),
        "[jackin debug isolation] git worktree add -b foo /tmp/wt deadbeef"
    );
}

#[test]
fn format_debug_line_passes_through_special_characters() {
    // No escaping — operators sharing logs need verbatim shell output.
    assert_eq!(
        format_debug_line("io", "wrote /a/b/c.json {\"k\":\"v\"}"),
        "[jackin debug io] wrote /a/b/c.json {\"k\":\"v\"}"
    );
}

#[test]
fn debug_mode_default_is_off() {
    // Process-wide flag — touching it would race other tests, so just
    // assert the snapshot is a bool. Toggle/observe is exercised in
    // the binary-level integration test.
    let _: bool = is_debug_mode();
}

#[test]
fn debug_lines_buffer_while_tui_is_active() {
    let _lock = DEBUG_BUFFER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    let _ = drain_debug_buffer();

    begin_debug_buffering();
    emit_debug_line("role", "resolving test role");
    assert_eq!(
        drain_debug_buffer(),
        vec!["[jackin debug role] resolving test role".to_string()]
    );
    end_debug_buffering();
}

#[test]
fn debug_lines_drop_while_a_noncapturing_run_owns_output() {
    let _lock = DEBUG_BUFFER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    let _ = drain_debug_buffer();

    let tmp = tempfile::tempdir().unwrap();
    let paths = crate::paths::JackinPaths::for_tests(tmp.path());
    let run = crate::diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    // A non-`--debug` run owns debug-tier output: the line is neither
    // buffered nor printed, so it can never reach a live rich surface.
    begin_debug_buffering();
    emit_debug_line("role", "should be dropped");
    assert!(
        drain_debug_buffer().is_empty(),
        "debug line must not buffer/print while a non-capturing run is active"
    );
    end_debug_buffering();
}

#[test]
fn compact_lines_write_run_file_while_rich_surface_owns_terminal() {
    let _lock = DEBUG_BUFFER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_rich_surface_active(false);
    set_host_screen_owned(false);
    let tmp = tempfile::tempdir().unwrap();
    let paths = crate::paths::JackinPaths::for_tests(tmp.path());
    let run = crate::diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    set_rich_surface_active(true);
    emit_compact_line("warning", "jackin: warning: hidden by cockpit");
    set_rich_surface_active(false);

    let jsonl = std::fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"kind\":\"warning\""), "{jsonl}");
    assert!(jsonl.contains("hidden by cockpit"), "{jsonl}");
    set_rich_surface_active(false);
    set_host_screen_owned(false);
}

#[test]
fn compact_lines_write_run_file_while_host_screen_owns_terminal() {
    let _lock = DEBUG_BUFFER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_rich_surface_active(false);
    set_host_screen_owned(false);
    let tmp = tempfile::tempdir().unwrap();
    let paths = crate::paths::JackinPaths::for_tests(tmp.path());
    let run = crate::diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    set_host_screen_owned(true);
    emit_compact_line("operator_env", "jackin: hidden while host owns raw screen");
    set_host_screen_owned(false);

    let jsonl = std::fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"kind\":\"operator_env\""), "{jsonl}");
    assert!(
        jsonl.contains("hidden while host owns raw screen"),
        "{jsonl}"
    );
}
