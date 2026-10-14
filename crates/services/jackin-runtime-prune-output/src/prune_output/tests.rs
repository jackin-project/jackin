// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn pending_parts_balances() {
    let (prefix, dots) = pending_parts("Remove", "container-1");
    assert!(prefix.starts_with("Remove"));
    assert!(prefix.ends_with("container-1"));
    assert!(dots.chars().all(|c| c == '.'));
    assert!(dots.len() >= 3);
}
