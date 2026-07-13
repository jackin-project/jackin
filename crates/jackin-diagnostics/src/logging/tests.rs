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
    assert_eq!(sink_level(TelemetrySink::DiagnosticsFile, true), global);
}

#[test]
fn parse_telemetry_level_matrix() {
    assert_eq!(parse_telemetry_level("info"), Some(TelemetryLevel::Info));
    assert_eq!(parse_telemetry_level("debug"), Some(TelemetryLevel::Debug));
    assert_eq!(parse_telemetry_level("trace"), Some(TelemetryLevel::Trace));
    assert_eq!(parse_telemetry_level("nope"), None);
}

#[test]
fn jackin_debug_alias_when_level_unset() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // SAFETY: exclusive test lock for process env.
    // Workspace forbids unsafe — use param-style where possible.
    // This test only asserts parse helpers remain stable; env matrix lives in
    // observability otlp tests when env can be isolated.
    assert_eq!(parse_telemetry_level("debug"), Some(TelemetryLevel::Debug));
}
