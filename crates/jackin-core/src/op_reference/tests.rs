// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `op_reference`.
use super::*;

#[test]
fn parse_op_reference_three_segments() {
    let parts = parse_op_reference("op://Vault/Item/field").unwrap();
    assert_eq!(parts.vault, "Vault");
    assert_eq!(parts.item, "Item");
    assert_eq!(parts.section, None);
    assert_eq!(parts.field, "field");
}

#[test]
fn parse_op_reference_handles_section_in_four_segments() {
    let parts = parse_op_reference("op://Personal/Item/Auth/password").unwrap();
    assert_eq!(parts.vault, "Personal");
    assert_eq!(parts.item, "Item");
    assert_eq!(parts.section, Some("Auth".to_owned()));
    assert_eq!(parts.field, "password");
}

#[test]
fn parse_op_reference_strips_query_suffix() {
    let parts = parse_op_reference("op://Vault/Item/token?attribute=otp").unwrap();
    assert_eq!(parts.field, "token");
    assert_eq!(parts.section, None);

    let parts = parse_op_reference("op://Vault/Item/Auth/key?ssh-format=openssh").unwrap();
    assert_eq!(parts.section, Some("Auth".to_owned()));
    assert_eq!(parts.field, "key");
}

#[test]
fn parse_op_reference_invalid_segment_count() {
    assert!(parse_op_reference("plain").is_none());
    assert!(parse_op_reference("op://only/two").is_none());
    assert!(parse_op_reference("op://a/b/c/d/e").is_none());
    assert!(parse_op_reference("op://").is_none());
    assert!(parse_op_reference("op:////").is_none());
    assert!(parse_op_reference("op://vault//field").is_none());
}

#[test]
fn op_reference_parts_manual_delete_hint_renders_canonical_cli() {
    let parts = parse_op_reference("op://VAULT_UUID/ITEM_UUID/FIELD").unwrap();
    assert_eq!(
        parts.manual_delete_hint().to_string(),
        "op item delete ITEM_UUID --vault VAULT_UUID",
    );
}
