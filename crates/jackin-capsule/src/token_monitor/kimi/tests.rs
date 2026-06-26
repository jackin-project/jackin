//! Tests for the parent module.
#[test]
fn kimi_token_reader_parses_wire_jsonl() {
    let line = r#"{"token_usage":{"input_other":500,"output":200,"input_cache_read":100,"input_cache_creation":50}}"#;
    let val: serde_json::Value = serde_json::from_str(line).unwrap();
    let usage = val.get("token_usage").unwrap();
    assert_eq!(
        usage.get("input_other").and_then(serde_json::Value::as_u64),
        Some(500)
    );
    assert_eq!(
        usage.get("output").and_then(serde_json::Value::as_u64),
        Some(200)
    );
    assert_eq!(
        usage
            .get("input_cache_read")
            .and_then(serde_json::Value::as_u64),
        Some(100)
    );
    assert_eq!(
        usage
            .get("input_cache_creation")
            .and_then(serde_json::Value::as_u64),
        Some(50)
    );
}
