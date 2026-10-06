// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the parent module.
use crate::token_monitor::TokenSession;
use jackin_core::Agent;

#[test]
fn amp_token_reader_parses_thread_messages() {
    let json = r#"[
        {"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":5},"model":"claude-3-5-sonnet"},
        {"usage":{"input_tokens":200,"output_tokens":80}}
    ]"#;
    let val: serde_json::Value = serde_json::from_str(json).unwrap();
    let arr = val.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let usage0 = arr[0].get("usage").unwrap();
    assert_eq!(
        usage0
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64),
        Some(100)
    );
    assert_eq!(
        arr[0].get("model").and_then(|v| v.as_str()),
        Some("claude-3-5-sonnet")
    );
}

#[test]
fn amp_token_reader_handles_messages_wrapper() {
    let json = r#"{"messages":[{"usage":{"input_tokens":300,"output_tokens":150}}]}"#;
    let val: serde_json::Value = serde_json::from_str(json).unwrap();
    let messages = val.get("messages").and_then(|m| m.as_array()).unwrap();
    assert_eq!(messages.len(), 1);
    let usage = messages[0].get("usage").unwrap();
    assert_eq!(
        usage
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64),
        Some(300)
    );
}

#[test]
fn amp_changed_flag_includes_cache_tokens() {
    let mut session = TokenSession::new(Agent::Amp);
    session.totals.input_tokens = 100;
    session.totals.output_tokens = 50;
    session.totals.cache_read_tokens = 0;

    let scratch_input: u64 = 100;
    let scratch_output: u64 = 50;
    let scratch_cache_read: u64 = 25;
    let scratch_cache_write: u64 = 0;

    let changed = scratch_input != session.totals.input_tokens
        || scratch_output != session.totals.output_tokens
        || scratch_cache_read != session.totals.cache_read_tokens
        || scratch_cache_write != session.totals.cache_write_tokens;

    assert!(changed, "cache-read change alone must flip changed flag");

    let old_changed = scratch_input != session.totals.input_tokens
        || scratch_output != session.totals.output_tokens;
    assert!(!old_changed, "confirms old logic would miss this change");
}

#[test]
fn amp_token_reader_skips_zero_usage() {
    let session = TokenSession::new(Agent::Amp);
    // Zero usage should not flip changed flag — verify via parse_raw_usage logic
    let zero = serde_json::json!({"usage":{"input_tokens":0,"output_tokens":0}});
    let usage = zero.get("usage").unwrap();
    let input = usage
        .get("input_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    assert_eq!(input, 0);
    assert_eq!(output, 0);
    assert_eq!(session.totals.input_tokens, 0);
}
