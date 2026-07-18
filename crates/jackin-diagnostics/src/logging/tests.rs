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
