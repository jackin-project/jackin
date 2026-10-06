// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the parent module.
use super::*;

#[test]
fn claude_token_reader_parses_jsonl_fields() {
    let line = r#"{"message":{"id":"msg_01","model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":5}},"requestId":"req_01","costUSD":0.42}"#;
    let parsed = parse_line(line).unwrap();
    assert_eq!(parsed.input_tokens, 100);
    assert_eq!(parsed.output_tokens, 50);
    assert_eq!(parsed.cache_creation_input_tokens, 10);
    assert_eq!(parsed.cache_read_input_tokens, 5);
    assert_eq!(parsed.cost_usd, Some(0.42));
    assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-6"));
}

#[test]
fn claude_token_reader_uses_costusd_when_present() {
    let line = r#"{"message":{"id":"msg_02","usage":{"input_tokens":1000,"output_tokens":500}},"costUSD":1.23}"#;
    let parsed = parse_line(line).unwrap();
    assert_eq!(parsed.cost_usd, Some(1.23));
}

#[test]
fn claude_token_reader_skips_sidechain() {
    let line = r#"{"isSidechain":true,"message":{"id":"msg_03","usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let parsed = parse_line(line).unwrap();
    assert!(parsed.is_sidechain);
}
