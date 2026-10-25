// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn accepts_conventional_names() {
    for name in ["FOO", "_private", "A1_B2", "x"] {
        assert!(is_valid_env_var_name(name), "rejected valid name {name}");
    }
}

#[test]
fn rejects_empty_and_digit_first_names() {
    for name in ["", "1ABC", "9_lives"] {
        assert!(
            !is_valid_env_var_name(name),
            "accepted invalid name {name:?}"
        );
    }
}

#[test]
fn rejects_punctuation_and_non_ascii_names() {
    for name in ["HAS-DASH", "HAS.DOT", "HAS SPACE", "ünïcode", "trailing_!"] {
        assert!(
            !is_valid_env_var_name(name),
            "accepted invalid name {name:?}"
        );
    }
}
