// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::base64_nopad_std;

#[test]
fn base64_empty() {
    assert_eq!(base64_nopad_std(b""), "");
}

#[test]
fn base64_hello() {
    assert_eq!(base64_nopad_std(b"hello"), "aGVsbG8=");
}
