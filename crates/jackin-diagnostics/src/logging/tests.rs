use super::{TelemetryLevel, TelemetrySink, parse_telemetry_level, sink_level, telemetry_level};

#[test]
fn sink_level_falls_back_to_global() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Without per-sink overrides, matches telemetry_level.
    let global = telemetry_level(true);
    assert_eq!(sink_level(TelemetrySink::OtlpSpans, true), global);
    assert_eq!(sink_level(TelemetrySink::OtlpLogs, true), global);
    assert_eq!(sink_level(TelemetrySink::Console, true), global);
}

#[test]
fn parse_telemetry_level_matrix() {
    assert_eq!(parse_telemetry_level("info"), Some(TelemetryLevel::Info));
    assert_eq!(parse_telemetry_level("debug"), Some(TelemetryLevel::Debug));
    assert_eq!(parse_telemetry_level("trace"), Some(TelemetryLevel::Trace));
    assert_eq!(parse_telemetry_level("nope"), None);
}

#[test]
fn formatted_debug_line_redacts_category_and_message() {
    let line =
        super::format_debug_line("client_token=category-canary", "db_password=message-canary");
    assert_eq!(line, "[jackin debug <redacted>] <redacted>");
    assert_eq!(
        super::format_debug_line("image", "ready"),
        "[jackin debug image] ready"
    );
}

#[test]
fn debug_and_notice_buffers_never_retain_raw_credentials() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(crate::run::active_run().is_none());
    assert!(super::drain_debug_buffer_for_test().is_empty());
    crate::terminal::set_rich_surface_active(false);
    super::begin_debug_buffering();
    super::emit_debug_line("image", "client_token=debug-canary");
    crate::terminal::set_rich_surface_active(true);
    super::emit_operator_notice("db_password=notice-canary");
    super::emit_compact_line("image", "access_key=compact-canary");
    crate::terminal::set_rich_surface_active(false);
    let lines = super::drain_debug_buffer_for_test();
    assert_eq!(
        lines,
        [
            "[jackin debug image] <redacted>",
            "<redacted>",
            "<redacted>"
        ]
    );
}

#[test]
fn terminal_sinks_redact_before_emission() {
    const CHILD_ENV: &str = "JACKIN_DIAGNOSTICS_REDACTION_TEST_CHILD";
    if let Ok(mode) = std::env::var(CHILD_ENV) {
        let canary = "client_token=terminal-secret-canary";
        match mode.as_str() {
            "debug" => super::emit_debug_line("image", canary),
            "notice" => super::emit_operator_notice(canary),
            "compact" => super::emit_compact_line("image", canary),
            "teardown" => super::emit_teardown_notice(canary),
            "deferred" => {
                crate::terminal::set_rich_surface_active(false);
                super::begin_debug_buffering();
                super::emit_debug_line("image", canary);
                crate::terminal::set_rich_surface_active(true);
                super::emit_operator_notice(canary);
                super::emit_compact_line("image", canary);
                crate::terminal::set_rich_surface_active(false);
                super::end_debug_buffering();
            }
            _ => panic!("unknown redaction test mode"),
        }
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for mode in ["debug", "notice", "compact", "teardown", "deferred"] {
        let output = runtime.block_on(async {
            tokio::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "logging::tests::terminal_sinks_redact_before_emission",
                    "--nocapture",
                ])
                .env(CHILD_ENV, mode)
                .output()
                .await
        });
        let output = output.unwrap();
        assert!(output.status.success(), "{mode}: child failed");
        let stderr = String::from_utf8(output.stderr).unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            !stderr.contains("terminal-secret-canary"),
            "{mode}: credential reached stderr"
        );
        assert!(
            !stdout.contains("terminal-secret-canary"),
            "{mode}: credential reached stdout"
        );
        assert!(
            stderr.contains("<redacted>"),
            "{mode}: redaction marker absent"
        );
    }
}
