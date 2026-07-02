use super::*;

#[test]
fn install_is_idempotent_via_set_global_dialog_sink_first_wins() {
    // Underlying `OnceLock::set` is idempotent: repeated calls are
    // silently dropped. Calling `install` twice must not panic or
    // replace the sink.
    install();
    install();
}

#[test]
fn sink_host_terminal_is_debug_mode_matches_diagnostics() {
    // The standalone dialog renderer calls `host.is_debug_mode()`
    // unconditionally during render; the SinkHostTerminal must
    // forward it to `jackin_diagnostics::is_debug_mode`.
    assert_eq!(
        SINK_HOST_TERMINAL.is_debug_mode(),
        jackin_diagnostics::is_debug_mode()
    );
}

#[test]
fn sink_forwards_through_standalone_dialog_renderers() {
    // Smoke-render into a thread that does not own a terminal. We
    // expect the renderer to short-circuit (no TTY) and return an
    // error — the goal here is that the sink impl wires through
    // without a panic, not that it produces a real dialog.
    let result = JackinStandaloneDialogSink.error_popup("title", "message");
    // Either Ok (silent short-circuit) or Err (no TTY) is acceptable.
    drop(result);
}
