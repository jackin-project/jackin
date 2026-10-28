// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use flate2::{Crc, read::ZlibDecoder};
use serde_json::{Value, json};
use tempfile::tempdir;

fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn run_measurement(args: impl IntoIterator<Item = OsString>) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_jackin-xtask"))
        .arg("measurement")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()
}

fn json_output(output: &Output) -> Result<Value, Box<dyn std::error::Error>> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_text_row(row: &Value, label: &str, text: &str) {
    assert_eq!(row["label"], label);
    assert_eq!(row["model"], "fixture-model");
    assert_eq!(row["characters"], 30);
    assert_eq!(row["bytes"], 31);
    assert_eq!(row["request"]["messages"][0]["content"], text);
}

fn append_path(args: &mut Vec<OsString>, path: &Path) {
    args.push(path.as_os_str().to_owned());
}

fn session_cost_output(transcripts: &[&Path], prices: Option<&Path>) -> std::io::Result<Output> {
    let mut args = vec![OsString::from("session-cost")];
    for transcript in transcripts {
        append_path(&mut args, transcript);
    }
    if let Some(prices) = prices {
        args.push(OsString::from("--prices"));
        append_path(&mut args, prices);
    }
    run_measurement(args)
}

fn write_jsonl(path: &Path, records: &[Value]) -> std::io::Result<()> {
    let mut contents = records
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    contents.push('\n');
    fs::write(path, contents)
}

fn assistant_record(
    session_id: Option<&str>,
    id: Option<&str>,
    model: &str,
    usage: Value,
) -> Value {
    let mut message = json!({"model": model, "usage": usage});
    if let Some(id) = id {
        message["id"] = json!(id);
    }
    let mut record = json!({"type": "assistant", "message": message});
    if let Some(session_id) = session_id {
        record["sessionId"] = json!(session_id);
    }
    record
}

fn cache_write_usage(five_minutes: u64, one_hour: u64, aggregate: u64) -> Value {
    json!({
        "input_tokens": 1,
        "cache_creation_input_tokens": aggregate,
        "cache_creation": {
            "ephemeral_5m_input_tokens": five_minutes,
            "ephemeral_1h_input_tokens": one_hour
        },
        "output_tokens": 1
    })
}

fn assert_session_cost_error(
    path: &Path,
    records: &[Value],
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write_jsonl(path, records)?;
    let output = session_cost_output(&[path], None)?;
    assert!(!output.status.success(), "{expected}");
    assert!(stderr(&output).contains(expected), "{}", stderr(&output));
    Ok(())
}

fn assert_session_cost_usage_errors(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for (name, usage, expected) in [
        ("false", json!(false), "usage must be an object"),
        ("zero", json!(0), "usage must be an object"),
        ("empty-string", json!(""), "usage must be an object"),
        ("empty-array", json!([]), "usage must be an object"),
        ("empty-object", json!({}), "missing or invalid input_tokens"),
        ("missing-input", json!({"output_tokens": 5}), "input_tokens"),
        (
            "missing-output",
            json!({"input_tokens": 5}),
            "output_tokens",
        ),
        (
            "negative-input",
            json!({"input_tokens": -1, "output_tokens": 0}),
            "input_tokens",
        ),
        (
            "breakdown-exceeds-aggregate",
            json!({
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_creation_input_tokens": 2,
                "cache_creation": {"ephemeral_5m_input_tokens": 3}
            }),
            "breakdown exceeds aggregate",
        ),
        (
            "empty-iterations",
            json!({"iterations": []}),
            "iterations must be a nonempty final usage array",
        ),
        (
            "nested-iterations",
            json!({"iterations": [{"iterations": [{"input_tokens": 1, "output_tokens": 1}]}]}),
            "nested usage iterations are invalid",
        ),
    ] {
        let path = directory.join(format!("{name}.jsonl"));
        assert_session_cost_error(
            &path,
            &[assistant_record(Some("s"), Some("id"), "model", usage)],
            expected,
        )?;
    }

    let iteration_overflow = json!({
        "iterations": [
            cache_write_usage(u64::MAX, 0, u64::MAX),
            cache_write_usage(0, 1, 1)
        ]
    });
    assert_session_cost_error(
        &directory.join("iteration-overflow.jsonl"),
        &[assistant_record(
            Some("s"),
            Some("id"),
            "model",
            iteration_overflow,
        )],
        "combined cache-write total overflow",
    )?;
    assert_session_cost_error(
        &directory.join("receipt-total-overflow.jsonl"),
        &[
            assistant_record(
                Some("s"),
                Some("max"),
                "model",
                cache_write_usage(u64::MAX, 0, u64::MAX),
            ),
            assistant_record(Some("s"), Some("one"), "model", cache_write_usage(0, 1, 1)),
        ],
        "combined cache-write total overflow",
    )?;
    Ok(())
}

fn assert_session_cost_stream_errors(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let malformed = directory.join("malformed.jsonl");
    fs::write(&malformed, b"\nnot-json")?;
    let output = session_cost_output(&[malformed.as_path()], None)?;
    assert!(!output.status.success());
    assert!(stderr(&output).contains(":2: invalid JSON"));

    let invalid_utf8 = directory.join("invalid-utf8.jsonl");
    fs::write(&invalid_utf8, b"{}\n\xff")?;
    let output = session_cost_output(&[invalid_utf8.as_path()], None)?;
    assert!(!output.status.success());
    assert!(stderr(&output).contains(":2: invalid UTF-8"));

    let records = |input: u64, output: u64| {
        assistant_record(
            Some("s"),
            Some("id"),
            "model",
            json!({"input_tokens": input, "output_tokens": output}),
        )
    };
    assert_session_cost_error(
        &directory.join("non-monotone.jsonl"),
        &[records(10, 12), records(10, 1)],
        "non-monotone duplicate usage",
    )?;
    assert_session_cost_error(
        &directory.join("model-conflict.jsonl"),
        &[
            records(1, 1),
            assistant_record(
                Some("s"),
                Some("id"),
                "other-model",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        ],
        "response model changed for the same ID",
    )?;
    assert_session_cost_error(
        &directory.join("missing-id.jsonl"),
        &[
            assistant_record(Some("s"), None, "model", json!({})),
            assistant_record(
                Some("s"),
                Some("later"),
                "model",
                json!({"input_tokens": 10, "output_tokens": 12}),
            ),
        ],
        "usage-bearing response has no message ID",
    )?;

    let ttl_breakdown = |five_minutes: u64, one_hour: u64| {
        json!({
            "input_tokens": 1,
            "cache_creation_input_tokens": 2,
            "cache_creation": {
                "ephemeral_5m_input_tokens": five_minutes,
                "ephemeral_1h_input_tokens": one_hour
            },
            "output_tokens": 1
        })
    };
    assert_session_cost_error(
        &directory.join("ttl-conflict.jsonl"),
        &[
            assistant_record(Some("s"), Some("id"), "model", ttl_breakdown(2, 0)),
            assistant_record(Some("s"), Some("id"), "model", ttl_breakdown(1, 1)),
        ],
        "conflicting cache TTL breakdown",
    )?;
    Ok(())
}

fn write_session_cost_pricing_fixture(
    directory: &Path,
) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let transcript = directory.join("receipts.jsonl");
    let compacted = assistant_record(
        Some("s"),
        Some("compacted"),
        "compacted-model",
        json!({
            "input_tokens": 100,
            "output_tokens": 10,
            "iterations": [
                {"type": "compaction", "input_tokens": 200, "output_tokens": 20},
                {"type": "message", "input_tokens": 100, "output_tokens": 10}
            ]
        }),
    );
    let mut records = vec![
        assistant_record(
            Some("s"),
            Some("partial-ttl"),
            "rated-unknown",
            json!({
                "input_tokens": 10,
                "cache_read_input_tokens": 4,
                "cache_creation": {
                    "ephemeral_5m_input_tokens": 70,
                    "ephemeral_1h_input_tokens": 30
                },
                "cache_creation_input_tokens": 200,
                "output_tokens": 6
            }),
        ),
        assistant_record(
            Some("s"),
            Some("untimed"),
            "untimed-model",
            json!({
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_creation_input_tokens": 100
            }),
        ),
        assistant_record(
            Some("s"),
            Some("partitioned"),
            "partitioned-model",
            json!({
                "input_tokens": 1,
                "output_tokens": 2,
                "cache_creation_input_tokens": 100,
                "cache_creation": {
                    "ephemeral_5m_input_tokens": 70,
                    "ephemeral_1h_input_tokens": 30
                }
            }),
        ),
        compacted.clone(),
        compacted,
    ];
    records.extend([
        assistant_record(
            Some("s"),
            Some("free"),
            "free-model",
            json!({"input_tokens": 2, "output_tokens": 3}),
        ),
        assistant_record(
            Some("s"),
            Some("unpriced"),
            "unpriced-model",
            json!({"input_tokens": 1, "output_tokens": 1}),
        ),
        assistant_record(
            Some("s"),
            Some("null-rate"),
            "null-priced-model",
            json!({"input_tokens": 1, "output_tokens": 1}),
        ),
    ]);
    write_jsonl(&transcript, &records)?;

    let prices = directory.join("prices.json");
    fs::write(
        &prices,
        json!({
            "rated-unknown": {
                "input": 1, "cache_read": 1, "cache_write_5m": 1,
                "cache_write_1h": 1, "output": 1
            },
            "untimed-model": {
                "input": 1, "cache_read": 1, "cache_write_5m": 1,
                "cache_write_1h": 1, "output": 1
            },
            "partitioned-model": {
                "input": 1, "cache_read": 1, "cache_write_5m": 1,
                "cache_write_1h": 1, "output": 1
            },
            "free-model": {
                "input": 0.0,
                "cache_read": 0.0,
                "cache_write_5m": 0.0,
                "cache_write_1h": 0.0,
                "output": 0.0
            },
            "null-priced-model": null
        })
        .to_string(),
    )?;
    Ok((transcript, prices))
}

#[test]
fn count_text_file_and_labeled_samples_preserve_unicode_and_input_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "μ\r\nexact without final newline";
    let direct = json_output(&run_measurement(strings(&[
        "count-tokens",
        "--model",
        "fixture-model",
        "text",
        text,
    ]))?)?;
    assert_text_row(&direct[0], "text", text);

    let directory = tempdir().unwrap();
    let text_path = directory.path().join("fixture.txt");
    fs::write(&text_path, text.as_bytes())?;
    let mut file_args = strings(&["count-tokens", "--model", "fixture-model", "file"]);
    append_path(&mut file_args, &text_path);
    let from_file = json_output(&run_measurement(file_args)?)?;
    assert_text_row(&from_file[0], &text_path.display().to_string(), text);

    let samples_path = directory.path().join("samples.json");
    fs::write(
        &samples_path,
        json!([{"label": "unicode", "text": text}]).to_string(),
    )?;
    let mut sample_args = strings(&["count-tokens", "--model", "fixture-model", "samples"]);
    append_path(&mut sample_args, &samples_path);
    let from_samples = json_output(&run_measurement(sample_args)?)?;
    assert_text_row(&from_samples[0], "unicode", text);

    let invalid_path = directory.path().join("invalid.txt");
    fs::write(&invalid_path, [b'o', b'k', b'\n', 0xff])?;
    let mut invalid_args = strings(&["count-tokens", "--model", "fixture-model", "file"]);
    append_path(&mut invalid_args, &invalid_path);
    let invalid = run_measurement(invalid_args)?;
    assert!(!invalid.status.success());
    assert!(stderr(&invalid).contains("not valid UTF-8"));
    Ok(())
}

#[test]
fn image_dimension_batches_validate_bounds_before_output() -> Result<(), Box<dyn std::error::Error>>
{
    let valid_args = strings(&[
        "image-tokens",
        "--model",
        "fixture-model",
        "4096x1",
        "1X4096",
    ]);
    let valid = json_output(&run_measurement(valid_args)?)?;
    assert_eq!(valid[0]["dimensions"], "4096x1");
    assert_eq!(valid[1]["dimensions"], "1X4096");
    for row in [&valid[0], &valid[1]] {
        assert_eq!(row["model"], "fixture-model");
        assert!(row["png_bytes"].as_u64().is_some_and(|bytes| bytes > 0));
        assert_eq!(
            row["request"]["messages"][0]["content"][0]["source"]["media_type"],
            "image/png"
        );
    }

    for dimension in ["0x10", "1x2x3", "4097x1"] {
        let invalid = run_measurement(strings(&[
            "image-tokens",
            "--model",
            "fixture-model",
            dimension,
        ]))?;
        assert!(!invalid.status.success(), "{dimension}");
        assert!(invalid.stdout.is_empty(), "{dimension}");
    }

    let mixed = run_measurement(strings(&[
        "image-tokens",
        "--model",
        "fixture-model",
        "1x1",
        "4097x1",
    ]))?;
    assert!(!mixed.status.success());
    assert!(mixed.stdout.is_empty());
    Ok(())
}

#[test]
fn session_cost_uses_latest_receipt_with_session_and_file_scopes()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir().unwrap();
    let first_path = directory.path().join("first.jsonl");
    let second_path = directory.path().join("second.jsonl");
    let repeated_record = assistant_record(
        Some("session-repeat"),
        Some("repeated-id"),
        "model-a",
        json!({"input_tokens": 1, "output_tokens": 1}),
    );
    let mut first_records = vec![
        assistant_record(Some("session-a"), None, "ignored-model", Value::Null),
        assistant_record(
            Some("session-a"),
            Some("shared-id"),
            "model-a",
            json!({
                "input_tokens": 3,
                "cache_read_input_tokens": 1,
                "cache_creation_input_tokens": 2,
                "cache_creation": {"ephemeral_5m_input_tokens": 2},
                "output_tokens": 2
            }),
        ),
        assistant_record(
            None,
            Some("file-id"),
            "model-a",
            json!({"input_tokens": 1, "output_tokens": 1}),
        ),
    ];
    first_records.extend([
        repeated_record.clone(),
        repeated_record.clone(),
        repeated_record,
    ]);
    write_jsonl(&first_path, &first_records)?;
    write_jsonl(
        &second_path,
        &[
            assistant_record(
                Some("session-a"),
                Some("shared-id"),
                "model-a",
                json!({
                    "input_tokens": 5,
                    "cache_read_input_tokens": 2,
                    "cache_creation_input_tokens": 3,
                    "cache_creation": {"ephemeral_5m_input_tokens": 3},
                    "output_tokens": 4
                }),
            ),
            assistant_record(
                Some("session-b"),
                Some("shared-id"),
                "model-b",
                json!({
                    "input_tokens": 7,
                    "cache_read_input_tokens": 1,
                    "cache_creation_input_tokens": 4,
                    "cache_creation": {"ephemeral_1h_input_tokens": 4},
                    "output_tokens": 6
                }),
            ),
            assistant_record(
                None,
                Some("file-id"),
                "model-a",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        ],
    )?;

    let report = json_output(&session_cost_output(
        &[first_path.as_path(), second_path.as_path()],
        None,
    )?)?;
    assert_eq!(report["unique_responses"], 5);
    assert_eq!(report["duplicate_records"], 3);
    assert!(report["tokens_by_model"].get("ignored-model").is_none());
    assert_eq!(
        report["tokens_by_model"]["model-a"],
        json!({
            "input": 8,
            "cache_read": 2,
            "cache_write_5m": 3,
            "cache_write_1h": 0,
            "cache_write_unknown": 0,
            "output": 7
        })
    );
    assert_eq!(
        report["tokens_by_model"]["model-b"],
        json!({
            "input": 7,
            "cache_read": 1,
            "cache_write_5m": 0,
            "cache_write_1h": 4,
            "cache_write_unknown": 0,
            "output": 6
        })
    );
    Ok(())
}

#[test]
fn session_cost_rejects_falsey_usage_and_invalid_receipt_streams()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir().unwrap();
    assert_session_cost_usage_errors(directory.path())?;
    assert_session_cost_stream_errors(directory.path())?;
    Ok(())
}

#[test]
fn session_cost_normalizes_iterations_ttls_and_exact_model_prices()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir().unwrap();
    let (transcript, prices) = write_session_cost_pricing_fixture(directory.path())?;

    let report = json_output(&session_cost_output(
        &[transcript.as_path()],
        Some(prices.as_path()),
    )?)?;
    assert_eq!(report["unique_responses"], 7);
    assert_eq!(report["duplicate_records"], 1);
    assert_eq!(
        report["tokens_by_model"]["rated-unknown"],
        json!({
            "input": 10,
            "cache_read": 4,
            "cache_write_5m": 70,
            "cache_write_1h": 30,
            "cache_write_unknown": 100,
            "output": 6
        })
    );
    assert_eq!(
        report["tokens_by_model"]["untimed-model"],
        json!({
            "input": 0,
            "cache_read": 0,
            "cache_write_5m": 0,
            "cache_write_1h": 0,
            "cache_write_unknown": 100,
            "output": 0
        })
    );
    assert_eq!(
        report["tokens_by_model"]["partitioned-model"],
        json!({
            "input": 1,
            "cache_read": 0,
            "cache_write_5m": 70,
            "cache_write_1h": 30,
            "cache_write_unknown": 0,
            "output": 2
        })
    );
    assert_eq!(
        report["tokens_by_model"]["compacted-model"],
        json!({"input": 300, "cache_read": 0, "cache_write_5m": 0, "cache_write_1h": 0, "cache_write_unknown": 0, "output": 30})
    );
    assert_eq!(
        report["list_price_estimate"]["rated-unknown"],
        json!({"usd": null, "reason": "cache write TTL unknown"})
    );
    assert_eq!(
        report["list_price_estimate"]["untimed-model"],
        json!({"usd": null, "reason": "cache write TTL unknown"})
    );
    assert_eq!(
        report["list_price_estimate"]["free-model"],
        json!({
            "usd": 0.0,
            "by_class": {
                "input": 0.0,
                "cache_read": 0.0,
                "cache_write_5m": 0.0,
                "cache_write_1h": 0.0,
                "output": 0.0
            }
        })
    );
    assert_eq!(
        report["list_price_estimate"]["unpriced-model"],
        json!({"usd": null, "reason": "no rate for exact model"})
    );
    assert_eq!(
        report["list_price_estimate"]["null-priced-model"],
        json!({"usd": null, "reason": "no rate for exact model"})
    );
    Ok(())
}

fn run_image_tokens(dimensions: &[&str]) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut args = strings(&["image-tokens", "--model", "fixture-model"]);
    args.extend(dimensions.iter().map(OsString::from));
    let output = json_output(&run_measurement(args)?)?;
    output
        .as_array()
        .cloned()
        .ok_or_else(|| std::io::Error::other("image output is an array").into())
}

fn assert_png_contract(
    row: &Value,
    dimension: &str,
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, usize), Box<dyn std::error::Error>> {
    assert_eq!(row["dimensions"], dimension);
    assert_eq!(row["model"], "fixture-model");
    assert_eq!(row["request"]["model"], "fixture-model");
    assert_eq!(row["request"]["messages"][0]["content"][0]["type"], "image");
    assert_eq!(
        row["request"]["messages"][0]["content"][0]["source"]["media_type"],
        "image/png"
    );
    assert_eq!(
        row["request"]["messages"][0]["content"][0]["source"]["type"],
        "base64"
    );
    let encoded = row["request"]["messages"][0]["content"][0]["source"]["data"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("PNG data is base64 text"))?;
    let png = BASE64.decode(encoded)?;
    assert_eq!(row["png_bytes"], png.len());
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

    let mut offset = 8;
    let mut chunk_types = Vec::new();
    let mut compressed = Vec::new();
    while offset < png.len() {
        assert!(png.len() - offset >= 12);
        let length = u32::from_be_bytes(png[offset..offset + 4].try_into()?) as usize;
        offset += 4;
        let kind: [u8; 4] = png[offset..offset + 4].try_into()?;
        offset += 4;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| std::io::Error::other("PNG chunk length overflow"))?;
        assert!(end <= png.len().saturating_sub(4));
        let payload = &png[offset..end];
        offset = end;
        let actual_crc = u32::from_be_bytes(png[offset..offset + 4].try_into()?);
        offset += 4;
        let mut crc = Crc::new();
        crc.update(&kind);
        crc.update(payload);
        assert_eq!(crc.sum(), actual_crc, "CRC for {kind:?}");
        chunk_types.push(kind);
        match &kind {
            b"IHDR" => {
                assert_eq!(payload.len(), 13);
                assert_eq!(u32::from_be_bytes(payload[0..4].try_into()?), width);
                assert_eq!(u32::from_be_bytes(payload[4..8].try_into()?), height);
                assert_eq!(&payload[8..], &[8, 2, 0, 0, 0]);
            }
            b"IDAT" => compressed.extend_from_slice(payload),
            b"IEND" => assert!(payload.is_empty()),
            _ => {
                return Err(std::io::Error::other(format!("unexpected PNG chunk {kind:?}")).into());
            }
        }
    }
    assert_eq!(chunk_types, [*b"IHDR", *b"IDAT", *b"IEND"]);

    let mut decoder = ZlibDecoder::new(compressed.as_slice());
    let mut raw = Vec::new();
    decoder.read_to_end(&mut raw)?;
    let row_bytes = width as usize * 3;
    assert_eq!(raw.len(), (row_bytes + 1) * height as usize);
    assert_eq!(&raw[..5], &[0, 0, 1, 2, 3]);
    let row_stride = row_bytes + 1;
    assert_eq!(raw[row_stride], 0);
    assert_eq!(&raw[row_stride + 1..row_stride + 4], &[7, 8, 9]);
    for row_index in 0..height as usize {
        assert_eq!(raw[row_index * row_stride], 0);
    }
    Ok((png, raw.len()))
}

#[test]
fn image_tokens_emit_valid_deterministic_pngs_without_batch_cap()
-> Result<(), Box<dyn std::error::Error>> {
    let dimensions = ["280x140", "280x140", "4096x4096", "2048x2048", "2048x2048"];
    let rows = run_image_tokens(&dimensions)?;
    assert_eq!(rows.len(), dimensions.len());
    let (first, _) = assert_png_contract(&rows[0], dimensions[0], 280, 140)?;
    let (repeated, _) = assert_png_contract(&rows[1], dimensions[1], 280, 140)?;
    assert_eq!(rows[0]["png_bytes"], rows[1]["png_bytes"]);
    assert_eq!(first, repeated);
    assert_png_contract(&rows[2], dimensions[2], 4096, 4096)?;
    let (_, first_large_raw_len) = assert_png_contract(&rows[3], dimensions[3], 2048, 2048)?;
    let (_, second_large_raw_len) = assert_png_contract(&rows[4], dimensions[4], 2048, 2048)?;
    let large_batch_raw_len = first_large_raw_len + second_large_raw_len;
    assert_eq!(large_batch_raw_len, 25_169_920);
    assert!(large_batch_raw_len > 16 * 1024 * 1024);
    Ok(())
}
