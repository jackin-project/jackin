// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `prompt`.
use super::prompt_choice_from;
use std::io::Cursor;

#[test]
fn prompt_choice_retries_empty_input() {
    let mut input = Cursor::new(b"\n3\n".as_slice());
    let mut output = Vec::new();

    let choice = prompt_choice_from(
        "Pick one",
        &["one", "two", "three"],
        &mut input,
        &mut output,
    )
    .unwrap();

    assert_eq!(choice, 2);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("Invalid choice \"\""));
    assert_eq!(output.matches("Choose [1/3]:").count(), 2);
}

#[test]
fn prompt_choice_retries_out_of_range_input() {
    let mut input = Cursor::new(b"4\n2\n".as_slice());
    let mut output = Vec::new();

    let choice = prompt_choice_from("Pick one", &["one", "two"], &mut input, &mut output).unwrap();

    assert_eq!(choice, 1);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("Invalid choice \"4\""));
    assert_eq!(output.matches("Choose [1/2]:").count(), 2);
}

#[test]
fn prompt_choice_reports_closed_input() {
    let mut input = Cursor::new([].as_slice());
    let mut output = Vec::new();

    let err = prompt_choice_from("Pick one", &["one"], &mut input, &mut output).unwrap_err();

    assert!(err.to_string().contains("input closed before a choice"));
}
