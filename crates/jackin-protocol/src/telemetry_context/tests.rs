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
