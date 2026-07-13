// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the parent module.
use super::*;

#[test]
fn cumulative_token_count_takes_last_value_not_sum() {
    // Two cumulative `total_token_usage` lines: the session total is the LAST
    // value (the counter is monotonic), never their sum. Regression guard for
    // the prior double-count, where each poll re-added the running total.
    let line1 = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":50}}}}"#;
    let line2 = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"output_tokens":90}}}}"#;
    let mut acc = Acc::default();
    apply_line(line1, &mut acc);
    apply_line(line2, &mut acc);
    assert_eq!(acc.cumulative, Some((200, 90, 0)));
    assert_eq!(acc.headless, (0, 0, 0));
    assert!(acc.seen);
}

#[test]
fn headless_usage_lines_sum() {
    let mut acc = Acc::default();
    apply_line(
        r#"{"usage":{"input_tokens":300,"output_tokens":100},"costUSD":0.15}"#,
        &mut acc,
    );
    apply_line(
        r#"{"usage":{"input_tokens":50,"output_tokens":20}}"#,
        &mut acc,
    );
    assert_eq!(acc.cumulative, None);
    assert_eq!(acc.headless, (350, 120, 0));
    assert!(acc.has_cost);
    assert!((acc.cost - 0.15).abs() < f64::EPSILON);
}

#[test]
fn parse_raw_usage_handles_alternate_field_names() {
    let obj = serde_json::json!({
        "prompt_tokens": 50,
        "completion_tokens": 20,
        "cache_read_input_tokens": 10,
    });
    let (inp, out, cached) = parse_raw_usage(&obj);
    assert_eq!(inp, 50);
    assert_eq!(out, 20);
    assert_eq!(cached, 10);
}
