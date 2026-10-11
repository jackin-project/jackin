use super::*;

#[test]
fn json_round_trip_omits_absent_values() {
    let context = TelemetryContext::v1();
    let json = serde_json::to_string(&context).unwrap();
    assert_eq!(json, r#"{"v":1}"#);
    assert_eq!(
        serde_json::from_str::<TelemetryContext>(&json).unwrap(),
        context
    );
}

#[test]
fn json_rejects_unknown_correlation_fields() {
    let error = serde_json::from_str::<TelemetryContext>(r#"{"v":1,"baggage":"secret"}"#)
        .expect_err("unknown context fields must fail closed");
    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn serialized_trace_context_extracts_as_remote_parent() {
    let json = r#"{"v":1,"traceparent":"00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01","tracestate":"vendor=value"}"#;
    let decoded: TelemetryContext = serde_json::from_str(json).unwrap();
    let jackin_telemetry::propagation::ExtractOutcome::Parent(parent) =
        jackin_telemetry::propagation::extract(&decoded)
    else {
        panic!("serialized W3C context must extract as a parent")
    };
    assert!(parent.is_remote());
    assert!(parent.is_sampled());
}
