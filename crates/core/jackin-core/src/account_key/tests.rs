// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::account_key_hash;

#[test]
fn account_key_hash_matches_golden_wire_format() {
    assert_eq!(
        account_key_hash("codex", "alexey@example.com"),
        "sha256:aff7fdb5324fe69ac47a6a53a92cd78ebc40d8c2c4057e6113b4ab4d7a398364"
    );
}

#[test]
fn account_key_hash_separator_prevents_field_boundary_collision() {
    assert_ne!(account_key_hash("ab", "c"), account_key_hash("a", "bc"));
}

#[test]
fn account_key_hash_separator_disambiguates_empty_components() {
    assert_ne!(account_key_hash("", "x"), account_key_hash("x", ""));
}

#[test]
fn account_key_hash_is_provider_namespaced() {
    assert_ne!(
        account_key_hash("codex", "alexey@example.com"),
        account_key_hash("claude", "alexey@example.com")
    );
}
