// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn braced_reference_extracts_name() {
    assert_eq!(parse_host_ref("${DATABASE_URL}"), Some("DATABASE_URL"));
}

#[test]
fn bare_reference_extracts_name() {
    assert_eq!(parse_host_ref("$DATABASE_URL"), Some("DATABASE_URL"));
}

#[test]
fn rejects_malformed_references() {
    for value in [
        "",
        "$",
        "${}",
        "${1BAD}",
        "${WITH SPACE}",
        "${UNCLOSED",
        "$9LIVES",
    ] {
        assert_eq!(parse_host_ref(value), None, "unexpected parse for {value}");
    }
}

#[test]
fn plain_names_are_not_references() {
    for value in ["DATABASE_URL", "FOO}"] {
        assert_eq!(parse_host_ref(value), None, "unexpected parse for {value}");
    }
}
