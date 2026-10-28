// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::OsString,
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

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
