// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{fs, io::Read};

use serde_json::{Value, json};
use tempfile::tempdir;

use super::{
    CountInput, Counts, collect_receipts, count_request_body, count_request_result, crc32,
    load_count_samples, normalize_usage, parse_dimension_args, parse_dimensions, png_raw_len,
    price_model, synthetic_png,
};

fn receipt(id: &str, session: &str, model: &str, usage: Value) -> Value {
    json!({
        "type": "assistant",
        "sessionId": session,
        "message": { "id": id, "model": model, "usage": usage }
    })
}

fn usage(input: u64, output: u64) -> Value {
    json!({"input_tokens": input, "output_tokens": output})
}

fn collect(
    rows: &[Value],
) -> anyhow::Result<(usize, usize, std::collections::BTreeMap<String, Counts>)> {
    let directory = tempdir()?;
    let path = directory.path().join("receipts.jsonl");
    let content = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, content)?;
    collect_receipts(&[path])
}

fn collect_bytes(
    content: &[u8],
) -> anyhow::Result<(usize, usize, std::collections::BTreeMap<String, Counts>)> {
    let directory = tempdir()?;
    let path = directory.path().join("receipts.jsonl");
    fs::write(&path, content)?;
    collect_receipts(&[path])
}

#[test]
fn offline_text_request_keeps_unicode_and_crlf_exact() {
    let content = "μ\r\nexact without final newline";
    let request = count_request_body("pinned-model", json!(content));
    assert_eq!(request["messages"][0]["content"], content);
    assert_eq!(
        content.len(),
        request["messages"][0]["content"].as_str().unwrap().len()
    );
}

#[test]
fn explicit_text_file_and_labeled_samples_preserve_input_bytes() {
    let text = "μ\r\nexact without final newline";
    let direct = load_count_samples(CountInput::Text {
        value: text.to_owned(),
    })
    .unwrap();
    assert_eq!(direct, [("text".to_owned(), text.to_owned())]);

    let directory = tempdir().unwrap();
    let text_path = directory.path().join("fixture.txt");
    fs::write(&text_path, text.as_bytes()).unwrap();
    let from_file = load_count_samples(CountInput::File {
        path: text_path.clone(),
    })
    .unwrap();
    assert_eq!(
        from_file,
        [(text_path.display().to_string(), text.to_owned())]
    );

    let samples_path = directory.path().join("samples.json");
    fs::write(
        &samples_path,
        json!([{"label": "unicode", "text": text}]).to_string(),
    )
    .unwrap();
    let from_samples = load_count_samples(CountInput::Samples { path: samples_path }).unwrap();
    assert_eq!(from_samples, [("unicode".to_owned(), text.to_owned())]);
    assert_eq!(text.chars().count(), 30);
    assert_eq!(text.len(), 31);
}

#[test]
fn invalid_utf8_text_file_fails_visibly() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("invalid.txt");
    fs::write(&path, [b'o', b'k', b'\n', 0xff]).unwrap();
    let error = load_count_samples(CountInput::File { path }).unwrap_err();
    assert!(format!("{error:#}").contains("not valid UTF-8"));
}

#[test]
fn offline_count_request_does_not_read_an_api_key() {
    let request = count_request_body("pinned-model", json!("fixture"));
    let (field, result) =
        count_request_result(&request, false, "JACKIN_MEASUREMENT_TEST_MISSING_API_KEY").unwrap();
    assert_eq!(field, "request");
    assert_eq!(result, request);
}

#[test]
fn repeated_receipts_count_once_and_report_duplicate_records() {
    let item = receipt("one", "session", "fixture-model", usage(100, 9));
    let (unique, duplicates, totals) = collect(&[item.clone(), item.clone(), item]).unwrap();
    assert_eq!(unique, 1);
    assert_eq!(duplicates, 2);
    assert_eq!(totals["fixture-model"].input, 100);
    assert_eq!(totals["fixture-model"].output, 9);
}

#[test]
fn cumulative_streaming_counters_keep_the_latest_values() {
    let (unique, duplicates, totals) = collect(&[
        receipt("one", "session", "fixture-model", usage(1, 1)),
        receipt("one", "session", "fixture-model", usage(10, 12)),
    ])
    .unwrap();
    assert_eq!((unique, duplicates), (1, 1));
    assert_eq!(totals["fixture-model"].input, 10);
    assert_eq!(totals["fixture-model"].output, 12);
}

#[test]
fn null_usage_is_skipped_before_valid_receipt() {
    let mut null_usage = receipt("one", "session", "fixture-model", Value::Null);
    null_usage["message"].as_object_mut().unwrap().remove("id");
    let (unique, duplicates, totals) = collect(&[
        null_usage,
        receipt("one", "session", "fixture-model", usage(10, 12)),
    ])
    .unwrap();
    assert_eq!((unique, duplicates), (1, 0));
    assert_eq!(totals["fixture-model"].input, 10);
    assert_eq!(totals["fixture-model"].output, 12);
}

#[test]
fn falsey_non_null_usage_values_fail_instead_of_disappearing() {
    for invalid_usage in [json!({}), json!(false), json!(0), json!(""), json!([])] {
        let error =
            collect(&[receipt("one", "session", "fixture-model", invalid_usage)]).unwrap_err();
        assert!(format!("{error:#}").contains("usage"), "{error:#}");
    }

    let mut missing_id = receipt("one", "session", "fixture-model", json!({}));
    missing_id["message"].as_object_mut().unwrap().remove("id");
    let error = collect(&[
        missing_id,
        receipt("later", "session", "fixture-model", usage(10, 12)),
    ])
    .unwrap_err();
    assert!(format!("{error:#}").contains("no message ID"));
}

#[test]
fn transcript_streaming_reports_invalid_utf8_and_exact_line() {
    let error = collect_bytes(b"\nnot-json").unwrap_err();
    assert!(format!("{error:#}").contains("receipts.jsonl:2: invalid JSON"));

    let error = collect_bytes(b"{}\n\xff").unwrap_err();
    assert!(format!("{error:#}").contains("receipts.jsonl:2: invalid UTF-8"));
}

#[test]
fn non_monotone_streaming_receipt_is_rejected() {
    let error = collect(&[
        receipt("one", "session", "fixture-model", usage(10, 12)),
        receipt("one", "session", "fixture-model", usage(10, 1)),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("non-monotone duplicate usage"));
}

#[test]
fn sessions_remain_distinct_and_missing_session_is_scoped_to_file() {
    let directory = tempdir().unwrap();
    let first = directory.path().join("first.jsonl");
    let second = directory.path().join("second.jsonl");
    let mut missing_session = receipt("same", "ignored", "a", usage(1, 1));
    missing_session.as_object_mut().unwrap().remove("sessionId");
    let second_missing_session = missing_session.clone();
    fs::write(&first, format!("{missing_session}\n")).unwrap();
    fs::write(&second, format!("{second_missing_session}\n")).unwrap();
    let (unique, duplicates, totals) = collect_receipts(&[first, second]).unwrap();
    assert_eq!((unique, duplicates), (2, 0));
    assert_eq!(totals["a"].output, 2);

    let (_, _, sessions) = collect(&[
        receipt("same", "one", "a", usage(1, 1)),
        receipt("same", "two", "b", usage(1, 3)),
    ])
    .unwrap();
    assert_eq!(sessions["a"].output, 1);
    assert_eq!(sessions["b"].output, 3);
}

#[test]
fn cache_write_aggregate_is_partitioned_without_double_counting() {
    let counts = normalize_usage(&json!({
        "input_tokens": 1,
        "output_tokens": 2,
        "cache_creation_input_tokens": 100,
        "cache_creation": {
            "ephemeral_5m_input_tokens": 70,
            "ephemeral_1h_input_tokens": 30
        }
    }))
    .unwrap();
    assert_eq!(counts.cache_write_5m, 70);
    assert_eq!(counts.cache_write_1h, 30);
    assert_eq!(counts.cache_write_unknown, 0);
    assert_eq!(counts.values().iter().sum::<u64>(), 103);
}

#[test]
fn aggregate_cache_writes_without_ttl_stay_unknown() {
    let counts = normalize_usage(&json!({
        "input_tokens": 0,
        "output_tokens": 0,
        "cache_creation_input_tokens": 100
    }))
    .unwrap();
    assert_eq!(counts.cache_write_unknown, 100);
    let rates = serde_json::from_value(json!({
        "fixture-model": {
            "input": 1, "cache_read": 1, "cache_write_5m": 1,
            "cache_write_1h": 1, "output": 1
        }
    }))
    .unwrap();
    let prices = price_model("fixture-model", &counts, &rates).unwrap();
    assert!(prices["usd"].is_null());
    assert_eq!(prices["reason"], "cache write TTL unknown");
}

#[test]
fn absent_or_null_exact_model_rate_stays_unknown() {
    let counts = Counts::default();
    let null_rates = serde_json::from_value(json!({"fixture-model": null})).unwrap();
    let prices = price_model("fixture-model", &counts, &null_rates).unwrap();
    assert!(prices["usd"].is_null());
    assert_eq!(prices["reason"], "no rate for exact model");
}

#[test]
fn invalid_or_incomplete_receipts_fail_visibly() {
    let missing = normalize_usage(&json!({"output_tokens": 5})).unwrap_err();
    assert!(missing.to_string().contains("input_tokens"));
    let negative = normalize_usage(&json!({"input_tokens": -1, "output_tokens": 0})).unwrap_err();
    assert!(negative.to_string().contains("input_tokens"));
    let inconsistent = normalize_usage(&json!({
        "input_tokens": 0, "output_tokens": 0,
        "cache_creation_input_tokens": 2,
        "cache_creation": {"ephemeral_5m_input_tokens": 3}
    }))
    .unwrap_err();
    assert!(
        inconsistent
            .to_string()
            .contains("breakdown exceeds aggregate")
    );
}

#[test]
fn compaction_iterations_replace_top_level_usage_and_sum_once() {
    let combined = json!({
        "input_tokens": 100,
        "output_tokens": 10,
        "iterations": [
            {"type": "compaction", "input_tokens": 200, "output_tokens": 20},
            {"type": "message", "input_tokens": 100, "output_tokens": 10}
        ]
    });
    let counts = normalize_usage(&combined).unwrap();
    assert_eq!((counts.input, counts.output), (300, 30));

    let duplicated = receipt("one", "session", "fixture-model", combined);
    let (unique, duplicate_records, totals) = collect(&[duplicated.clone(), duplicated]).unwrap();
    assert_eq!((unique, duplicate_records), (1, 1));
    assert_eq!(totals["fixture-model"].output, 30);
}

#[test]
fn compaction_iterations_must_be_nonempty_and_not_nested() {
    drop(normalize_usage(&json!({"iterations": []})).unwrap_err());
    drop(
        normalize_usage(&json!({
            "iterations": [{"iterations": [{"input_tokens": 1, "output_tokens": 1}]}]
        }))
        .unwrap_err(),
    );
}

#[test]
fn combined_cache_write_total_overflow_is_reported_for_iterations_and_model_totals() {
    let max = u64::MAX;
    let iterations = json!({
        "input_tokens": 0,
        "output_tokens": 0,
        "iterations": [
            {
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_creation_input_tokens": max,
                "cache_creation": {"ephemeral_5m_input_tokens": max}
            },
            {
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_creation_input_tokens": 1,
                "cache_creation": {"ephemeral_1h_input_tokens": 1}
            }
        ]
    });
    let error = normalize_usage(&iterations).unwrap_err();
    assert!(format!("{error:#}").contains("combined cache-write total overflow"));

    let first = receipt(
        "same",
        "session",
        "fixture-model",
        json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "cache_creation_input_tokens": max,
            "cache_creation": {"ephemeral_5m_input_tokens": max}
        }),
    );
    let second = receipt(
        "other",
        "session",
        "fixture-model",
        json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "cache_creation_input_tokens": 1,
            "cache_creation": {"ephemeral_1h_input_tokens": 1}
        }),
    );
    let error = collect(&[first, second]).unwrap_err();
    assert!(format!("{error:#}").contains("combined cache-write total overflow"));
}

#[test]
fn response_model_conflicts_and_missing_ids_fail() {
    let conflict = collect(&[
        receipt("one", "session", "a", usage(1, 1)),
        receipt("one", "session", "b", usage(1, 2)),
    ])
    .unwrap_err();
    assert!(conflict.to_string().contains("model changed"));

    let mut missing = receipt("one", "session", "a", usage(1, 1));
    missing["message"].as_object_mut().unwrap().remove("id");
    let missing_id = collect(&[missing]).unwrap_err();
    assert!(format!("{missing_id:#}").contains("no message ID"));
}

#[test]
fn cache_ttl_conflict_for_same_receipt_fails() {
    let first = receipt(
        "one",
        "session",
        "fixture-model",
        json!({
            "input_tokens": 1,
            "output_tokens": 1,
            "cache_creation_input_tokens": 2,
            "cache_creation": {"ephemeral_5m_input_tokens": 2}
        }),
    );
    let second = receipt(
        "one",
        "session",
        "fixture-model",
        json!({
            "input_tokens": 1,
            "output_tokens": 1,
            "cache_creation_input_tokens": 2,
            "cache_creation": {
                "ephemeral_5m_input_tokens": 1,
                "ephemeral_1h_input_tokens": 1
            }
        }),
    );
    let error = collect(&[first, second]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("conflicting cache TTL breakdown")
    );
}

#[test]
fn exact_model_zero_rate_estimate_remains_zero() {
    let counts = normalize_usage(&usage(0, 0)).unwrap();
    let rates = serde_json::from_value(json!({
        "fixture-model": {
            "input": 0, "cache_read": 0, "cache_write_5m": 0,
            "cache_write_1h": 0, "output": 0
        }
    }))
    .unwrap();
    let prices = price_model("fixture-model", &counts, &rates).unwrap();
    assert_eq!(prices["usd"], 0.0);
}

#[test]
fn png_fixture_is_deterministic_valid_and_bounded() {
    let first = synthetic_png(280, 140).unwrap();
    assert_eq!(first, synthetic_png(280, 140).unwrap());
    assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(first[16..20].try_into().unwrap()), 280);
    assert_eq!(u32::from_be_bytes(first[20..24].try_into().unwrap()), 140);
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);

    let idat = 8 + 25;
    let compressed_len = u32::from_be_bytes(first[idat..idat + 4].try_into().unwrap()) as usize;
    let mut decoder = flate2::read::ZlibDecoder::new(&first[idat + 8..idat + 8 + compressed_len]);
    let mut raw = Vec::new();
    decoder.read_to_end(&mut raw).unwrap();
    assert_eq!(raw.len(), (280 * 3 + 1) * 140);
    assert_eq!(&raw[..5], &[0, 0, 1, 2, 3]);
    assert_eq!(raw[280 * 3 + 1], 0);
    assert_eq!(raw[280 * 3 + 2], 7);

    let maximum_raw_len = (4096 * 3 + 1) * 4096;
    assert_eq!(png_raw_len(4096, 4096).unwrap(), maximum_raw_len);
    let maximum = synthetic_png(4096, 4096).unwrap();
    assert_eq!(
        u32::from_be_bytes(maximum[16..20].try_into().unwrap()),
        4096
    );
    assert_eq!(
        u32::from_be_bytes(maximum[20..24].try_into().unwrap()),
        4096
    );
}

#[test]
fn multiple_large_pngs_are_processed_without_an_aggregate_limit() {
    let mut raw_total = 0usize;
    for _ in 0..2 {
        raw_total = raw_total
            .checked_add(png_raw_len(2048, 2048).unwrap())
            .unwrap();
        let image = synthetic_png(2048, 2048).unwrap();
        assert_eq!(u32::from_be_bytes(image[16..20].try_into().unwrap()), 2048);
        drop(image);
    }
    assert!(raw_total > 16 * 1024 * 1024);
}

#[test]
fn all_image_dimensions_are_validated_before_any_image_request() {
    drop(parse_dimension_args(vec!["1x1".to_owned(), "4097x1".to_owned()]).unwrap_err());
    assert_eq!(
        parse_dimension_args(vec!["4096x1".to_owned(), "1x4096".to_owned()]).unwrap(),
        [
            ("4096x1".to_owned(), 4096, 1),
            ("1x4096".to_owned(), 1, 4096)
        ]
    );
}

#[test]
fn dimensions_are_checked_and_accept_uppercase_x() {
    assert_eq!(parse_dimensions("280X140").unwrap(), (280, 140));
    drop(parse_dimensions("0x10").unwrap_err());
    drop(parse_dimensions("1x2x3").unwrap_err());
    drop(parse_dimensions("4097x1").unwrap_err());
}
