//! Tests for the parent module.
use super::*;

#[test]
fn codex_token_reader_computes_per_turn_delta() {
    let line1 = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":50}}}}"#;
    let line2 = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"output_tokens":90}}}}"#;
    let v1: serde_json::Value = serde_json::from_str(line1).unwrap();
    let v2: serde_json::Value = serde_json::from_str(line2).unwrap();
    assert_eq!(v1.get("type").and_then(|v| v.as_str()), Some("event_msg"));
    assert_eq!(v2.get("type").and_then(|v| v.as_str()), Some("event_msg"));
    let info2 = &v2["payload"]["info"]["total_token_usage"];
    let (inp, out, _, _) = parse_raw_usage(info2);
    assert_eq!(inp, 200);
    assert_eq!(out, 90);
}

#[test]
fn codex_token_reader_handles_headless_format() {
    let line = r#"{"usage":{"input_tokens":300,"output_tokens":100},"costUSD":0.15}"#;
    let val: serde_json::Value = serde_json::from_str(line).unwrap();
    assert!(val.get("usage").is_some());
    assert_eq!(
        val.get("costUSD").and_then(serde_json::Value::as_f64),
        Some(0.15)
    );
    let (inp, out, _, _) = parse_raw_usage(val.get("usage").unwrap());
    assert_eq!(inp, 300);
    assert_eq!(out, 100);
}

#[test]
fn parse_raw_usage_handles_alternate_field_names() {
    let obj = serde_json::json!({
        "prompt_tokens": 50,
        "completion_tokens": 20,
        "cache_read_input_tokens": 10,
        "reasoning_output_tokens": 5,
    });
    let (inp, out, cached, reasoning) = parse_raw_usage(&obj);
    assert_eq!(inp, 50);
    assert_eq!(out, 20);
    assert_eq!(cached, 10);
    assert_eq!(reasoning, 5);
}
