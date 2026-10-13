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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_codex_model_is_consumed_without_export() {
    let model = "gpt-4o-wire-private-model";
    let line = serde_json::json!({
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "model_name": model,
            "info": {
                "total_token_usage": {
                    "input_tokens": 120,
                    "output_tokens": 45,
                    "cached_input_tokens": 20
                }
            }
        }
    })
    .to_string();
    let mut acc = Acc::default();
    apply_line(&line, &mut acc);
    assert_eq!(acc.model.as_deref(), Some(model));
    assert!(
        crate::token_monitor::pricing::estimate_cost_usd(model, 120, 45, 20, 0).is_some(),
        "parsed model was not consumed by local pricing"
    );

    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");
    let current = crate::token_monitor::TokenTotals {
        input_tokens: 120,
        output_tokens: 45,
        cache_read_tokens: 20,
        model: acc.model,
        ..crate::token_monitor::TokenTotals::default()
    };
    crate::token_monitor::record_token_usage(
        jackin_core::Agent::Codex,
        &crate::token_monitor::TokenTotals::default(),
        &current,
    );
    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .expect("start validation operation");
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .expect("emit validation event");
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");
    assert!(
        testbed
            .wait_for_all_signals(std::time::Duration::from_secs(2))
            .await
    );
    assert!(
        testbed
            .metric_names()
            .iter()
            .any(|name| name == "gen_ai.client.token.usage")
    );
    assert_eq!(
        testbed.prohibited_value_violations(&[model]),
        Vec::<String>::new()
    );
    jackin_diagnostics::shutdown_capsule_tracing();
}
