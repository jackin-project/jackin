// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use crate::workflow_yaml::parse;

#[test]
fn mapping_parses_to_json_value() {
    let value = parse(b"title: hello\ncount: 3\n").expect("valid yaml parses");
    assert_eq!(
        value.get("title").and_then(serde_json::Value::as_str),
        Some("hello")
    );
    assert_eq!(
        value.get("count").and_then(serde_json::Value::as_i64),
        Some(3)
    );
}

#[test]
fn nested_mapping_preserves_structure() {
    let value = parse(b"outer:\n  inner: deep\n").expect("valid yaml parses");
    assert_eq!(
        value
            .pointer("/outer/inner")
            .and_then(serde_json::Value::as_str),
        Some("deep")
    );
}

#[test]
fn invalid_yaml_is_rejected() {
    parse(b"[\nunclosed").unwrap_err();
}

#[test]
fn oversize_input_is_rejected_before_parsing() {
    let oversize = vec![b'a'; 1024 * 1024 + 1];
    parse(&oversize).unwrap_err();
}
