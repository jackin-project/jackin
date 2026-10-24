// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde_json::{Map, json};

use super::{normalize, price};

#[test]
fn openai_paid_write_classes_are_disjoint_and_checked() {
    let normalized = normalize(
        "openai-responses",
        &json!({
            "input_tokens": 12_000,
            "output_tokens": 800,
            "input_tokens_details": {
                "cached_tokens": 4_000,
                "cache_write_tokens": 7_000,
                "cache_write_tokens_details": {"ttl_30m_tokens": 3_000, "ttl_12h_tokens": 4_000}
            },
            "output_tokens_details": {"reasoning_tokens": 300}
        }),
        Some("paid"),
    )
    .unwrap();
    assert_eq!(normalized["tokens"]["input"], 1_000);
    assert_eq!(normalized["tokens"]["cache_read"], 4_000);
    assert_eq!(normalized["tokens"]["cache_write_30m"], 3_000);
    assert_eq!(normalized["tokens"]["cache_write_12h"], 4_000);
    assert_eq!(normalized["tokens"]["output"], 800);
    assert_eq!(normalized["reasoning_output_tokens"], 300);
    assert!(
        normalized["reasoning_included_in_output"]
            .as_bool()
            .unwrap()
    );
    let estimate = price(
        &normalized,
        &serde_json::from_value(json!({
            "input": 2,
            "cache_read": 0.1,
            "cache_write_30m": 2.5,
            "cache_write_12h": 5,
            "output": 10
        }))
        .unwrap(),
    )
    .unwrap();
    assert!((estimate["usd"].as_f64().unwrap() - 0.0379).abs() < 1e-12);
    assert_eq!(estimate["by_class"]["input"], 0.002);
    assert_eq!(estimate["by_class"]["cache_read"], 0.0004);
    assert_eq!(estimate["by_class"]["cache_write_30m"], 0.0075);
    assert_eq!(estimate["by_class"]["cache_write_12h"], 0.02);
    assert_eq!(estimate["by_class"]["output"], 0.008);
    drop(
        normalize(
            "openai-responses",
            &json!({
                "input_tokens": 12_000,
                "output_tokens": 800,
                "input_tokens_details": {
                    "cached_tokens": 4_000,
                    "cache_write_tokens": 7_001,
                    "cache_write_tokens_details": {"ttl_30m_tokens": 3_000, "ttl_12h_tokens": 4_000}
                }
            }),
            Some("paid"),
        )
        .unwrap_err(),
    );
}

#[test]
fn openai_paid_writes_must_be_reported_or_explicitly_excluded() {
    let receipt = json!({
        "input_tokens": 10,
        "output_tokens": 0,
        "input_tokens_details": {"cached_tokens": 0}
    });
    drop(normalize("openai-responses", &receipt, Some("paid")).unwrap_err());
    assert_eq!(
        normalize("openai-responses", &receipt, Some("ordinary-input")).unwrap()["tokens"]["input"],
        10
    );
}

#[test]
fn gemini_interactions_keeps_thought_disjoint_and_checks_total() {
    let normalized = normalize(
        "gemini-interactions",
        &json!({
            "total_input_tokens": 62,
            "total_output_tokens": 171,
            "total_thought_tokens": 297,
            "total_tokens": 530
        }),
        None,
    )
    .unwrap();
    assert_eq!(normalized["tokens"]["input"], 62);
    assert_eq!(normalized["tokens"]["output"], 468);
    assert_eq!(normalized["reasoning_output_tokens"], 297);
    drop(
        normalize(
            "gemini-interactions",
            &json!({"total_input_tokens": 62, "total_output_tokens": 171}),
            None,
        )
        .unwrap_err(),
    );
    drop(
        normalize(
            "gemini-interactions",
            &json!({
                "total_input_tokens": 62,
                "total_cached_tokens": null,
                "total_output_tokens": 171,
                "total_thought_tokens": 0
            }),
            None,
        )
        .unwrap_err(),
    );
}

#[test]
fn reasoning_is_bounded_by_reported_output_and_unknown_rates_stay_unknown() {
    drop(
        normalize(
            "openai-responses",
            &json!({
                "input_tokens": 1,
                "output_tokens": 10,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens_details": {"reasoning_tokens": 11}
            }),
            Some("ordinary-input"),
        )
        .unwrap_err(),
    );
    let counts = normalize(
        "openai-responses",
        &json!({
            "input_tokens": 1,
            "output_tokens": 0,
            "input_tokens_details": {"cached_tokens": 0}
        }),
        Some("ordinary-input"),
    )
    .unwrap();
    let estimate = price(&counts, &Map::new()).unwrap();
    assert!(estimate["usd"].is_null());
    assert_eq!(estimate["reason"], "no exact rate for input");
}

#[test]
fn claude_thinking_is_optional_bounded_and_summed_for_split_iterations() {
    let missing = normalize(
        "claude-messages",
        &json!({"input_tokens": 1, "output_tokens": 10}),
        None,
    )
    .unwrap();
    assert!(missing["reasoning_output_tokens"].is_null());

    let over = normalize(
        "claude-messages",
        &json!({
            "input_tokens": 1,
            "output_tokens": 10,
            "output_tokens_details": {"thinking_tokens": 11}
        }),
        None,
    )
    .unwrap_err();
    assert!(format!("{over:#}").contains("thinking exceeds output total"));

    let split = normalize(
        "claude-messages",
        &json!({
            "input_tokens": 100,
            "output_tokens": 30,
            "iterations": [
                {"type": "compaction", "input_tokens": 200, "output_tokens": 20,
                 "output_tokens_details": {"thinking_tokens": 6}},
                {"type": "message", "input_tokens": 100, "output_tokens": 10,
                 "output_tokens_details": {"thinking_tokens": 4}}
            ]
        }),
        None,
    )
    .unwrap();
    assert_eq!(split["tokens"]["input"], 300);
    assert_eq!(split["tokens"]["output"], 30);
    assert_eq!(split["reasoning_output_tokens"], 10);

    let mixed = normalize(
        "claude-messages",
        &json!({
            "input_tokens": 100,
            "output_tokens": 30,
            "iterations": [
                {"type": "compaction", "input_tokens": 200, "output_tokens": 20,
                 "output_tokens_details": {"thinking_tokens": 6}},
                {"type": "message", "input_tokens": 100, "output_tokens": 10}
            ]
        }),
        None,
    )
    .unwrap();
    assert!(mixed["reasoning_output_tokens"].is_null());
}

#[test]
fn absent_and_null_rates_stay_unknown_while_invalid_rates_fail() {
    let normalized = normalize(
        "openai-responses",
        &json!({
            "input_tokens": 1,
            "output_tokens": 0,
            "input_tokens_details": {"cached_tokens": 0}
        }),
        Some("ordinary-input"),
    )
    .unwrap();

    let no_rate = price(&normalized, &Map::new()).unwrap();
    assert!(no_rate["usd"].is_null());
    assert_eq!(no_rate["reason"], "no exact rate for input");

    let null_rate = price(
        &normalized,
        &serde_json::from_value(json!({"input": null})).unwrap(),
    )
    .unwrap();
    assert!(null_rate["usd"].is_null());
    assert_eq!(null_rate["reason"], "no exact rate for input");

    for invalid_rate in [json!("2"), json!(-1)] {
        let rates = serde_json::from_value(json!({"input": invalid_rate})).unwrap();
        drop(price(&normalized, &rates).unwrap_err());
    }

    let zero_rate = price(
        &normalized,
        &serde_json::from_value(json!({"input": 0})).unwrap(),
    )
    .unwrap();
    assert_eq!(zero_rate["usd"], 0.0);
    let positive_rate = price(
        &normalized,
        &serde_json::from_value(json!({"input": 2})).unwrap(),
    )
    .unwrap();
    assert_eq!(positive_rate["usd"], 0.000002);

    let huge = normalize(
        "openai-responses",
        &json!({
            "input_tokens": u64::MAX,
            "output_tokens": 0,
            "input_tokens_details": {"cached_tokens": 0}
        }),
        Some("ordinary-input"),
    )
    .unwrap();
    let huge_rates = serde_json::from_value(json!({"input": 1.0e308})).unwrap();
    assert!(
        format!("{:#}", price(&huge, &huge_rates).unwrap_err()).contains("price estimate overflow")
    );
}

#[test]
fn historical_synthetic_claude_gemini_and_unknown_ttl_prices_match() {
    let claude = normalize(
        "claude-messages",
        &json!({
            "input_tokens": 100,
            "output_tokens": 10,
            "iterations": [
                {"type": "compaction", "input_tokens": 200, "output_tokens": 20},
                {"type": "message", "input_tokens": 100, "output_tokens": 10}
            ]
        }),
        None,
    )
    .unwrap();
    let claude_rates = serde_json::from_value(json!({
        "input": 2, "cache_read": 0.1, "cache_write_5m": 2.5,
        "cache_write_1h": 4, "output": 10
    }))
    .unwrap();
    let claude_price = price(&claude, &claude_rates).unwrap();
    assert!((claude_price["usd"].as_f64().unwrap() - 0.0009).abs() < 1e-12);

    let gemini = normalize(
        "gemini-interactions",
        &json!({
            "total_input_tokens": 62,
            "total_output_tokens": 171,
            "total_thought_tokens": 297,
            "total_tokens": 530
        }),
        None,
    )
    .unwrap();
    let gemini_rates =
        serde_json::from_value(json!({"input": 2, "cache_read": 0.1, "output": 10})).unwrap();
    let gemini_price = price(&gemini, &gemini_rates).unwrap();
    assert!((gemini_price["usd"].as_f64().unwrap() - 0.004804).abs() < 1e-12);

    let unknown = normalize(
        "claude-messages",
        &json!({
            "input_tokens": 1,
            "output_tokens": 10,
            "cache_creation_input_tokens": 100
        }),
        None,
    )
    .unwrap();
    let unknown_price = price(&unknown, &claude_rates).unwrap();
    assert!(unknown_price["usd"].is_null());
    assert_eq!(unknown_price["reason"], "cache write lifetime unknown");
}
