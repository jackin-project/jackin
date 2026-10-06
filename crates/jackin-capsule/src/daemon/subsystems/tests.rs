//! Focused contracts for daemon subsystem APIs.

#[test]
fn empty_control_and_clipboard_report_inactive() {
    let mux = crate::daemon::tests::single_pane_tab_mux();
    assert!(mux.control.dialog_top().is_none());
    assert!(!mux.clipboard.is_selecting());
}

#[test]
fn render_and_launch_accessors_preserve_constructor_state() {
    let mux = crate::daemon::tests::single_pane_tab_mux();
    assert_eq!(mux.render.terminal_size(), (24, 80));
    assert_eq!(mux.launch_env.config().role, "test-role");
}

#[test]
fn status_pr_and_usage_accessors_read_owned_state() {
    let mux = crate::daemon::tests::single_pane_tab_mux();
    let (container, role) = mux.status.container_identity();
    // Role comes from CapsuleConfig; container name is env/hostname-derived and
    // may be empty on bare host unit-test runs (no JACKIN_CONTAINER_NAME).
    let _ = container;
    assert_eq!(role, "test-role");
    assert_eq!(mux.pr_watch.context(), (None, None));
    drop(mux.usage.cache().account_snapshot_views());
}
