// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    let mut forwarded = None;
    JackinStandaloneDialogSink::error_popup_with_renderer(
        "title",
        "message",
        |title, message, host, version| {
            forwarded = Some((
                title.to_owned(),
                message.to_owned(),
                host.is_debug_mode(),
                version,
            ));
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(
        forwarded,
        Some((
            "title".to_owned(),
            "message".to_owned(),
            jackin_diagnostics::is_debug_mode(),
            env!("JACKIN_VERSION"),
        ))
    );
}
