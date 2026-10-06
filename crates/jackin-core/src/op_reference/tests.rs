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

#[test]
fn builds_references_only_from_single_path_component_ids() {
    assert_eq!(
        build_op_reference("vault-id", "item-id", Some("section-id"), "field-id"),
        Some("op://vault-id/item-id/section-id/field-id".to_owned())
    );
    assert_eq!(
        build_op_reference("vault-id", "item-id", None, "field-id"),
        Some("op://vault-id/item-id/field-id".to_owned())
    );
    assert_eq!(
        build_op_reference("vault/id", "item-id", None, "field-id"),
        None
    );
    assert_eq!(
        build_op_reference("vault-id", "item-id", Some("bad?section"), "field-id"),
        None
    );
}

#[test]
fn v1_breadcrumb_round_trip_preserves_literal_reserved_text_and_unicode() {
    let encoded = [
        encode_op_breadcrumb_segment("Team%2FBlue"),
        encode_op_breadcrumb_segment("Item/Name[Alt]"),
        encode_op_breadcrumb_segment("Deploy?Token"),
    ]
    .join("/");
    assert_eq!(encoded, "Team%252FBlue/Item%2FName%5BAlt%5D/Deploy%3FToken");
    assert_eq!(
        decode_op_breadcrumb_segment("Team%252FBlue"),
        Some("Team%2FBlue".to_owned())
    );
    assert_eq!(
        decode_op_breadcrumb_segment("Item%2FName%5BAlt%5D"),
        Some("Item/Name[Alt]".to_owned())
    );
    assert_eq!(decode_op_breadcrumb_segment("Deploy%ZZ"), None);
    assert_eq!(decode_op_breadcrumb_segment("Deploy%41"), None);
    assert_eq!(encode_op_breadcrumb_segment("Café"), "Café");
}

#[test]
fn legacy_breadcrumb_migration_treats_percent_as_literal_and_checks_structure() {
    assert_eq!(
        encode_legacy_op_breadcrumb(
            "Vault/Item/Team%2FBlue/Token?attribute=username",
            "op://vault-id/item-id/section-id/field-id?attribute=username"
        ),
        Some("Vault/Item/Team%252FBlue/Token?attribute=username".to_owned())
    );
    // Older snapshots often stored only the display path; the URI query is
    // still preserved by `op` and must not be fabricated into the breadcrumb.
    assert_eq!(
        encode_legacy_op_breadcrumb(
            "Vault/Item/Token",
            "op://vault-id/item-id/field-id?attribute=otp"
        ),
        Some("Vault/Item/Token".to_owned())
    );
    assert_eq!(
        encode_legacy_op_breadcrumb(
            "Vault/Item/Token?attribute=wrong",
            "op://vault-id/item-id/field-id?attribute=otp"
        ),
        None
    );
    assert_eq!(
        encode_legacy_op_breadcrumb("Vault/Item/Section/Token", "op://vault-id/item-id/field-id"),
        None
    );
}

#[test]
fn breadcrumb_parser_keeps_item_subtitle_and_attribute_suffix_distinct() {
    let parts =
        parse_op_breadcrumb_path("Personal/Key[work%2Fadmin]/Auth/Token?attribute=otp").unwrap();
    assert_eq!(parts.vault, "Personal");
    assert_eq!(parts.item, "Key");
    assert_eq!(parts.item_subtitle.as_deref(), Some("work/admin"));
    assert_eq!(parts.section.as_deref(), Some("Auth"));
    assert_eq!(parts.field, "Token");
    assert_eq!(parts.attribute_query.as_deref(), Some("?attribute=otp"));
    assert_eq!(
        display_op_breadcrumb_path("Personal/Key[work%2Fadmin]/Auth/Token?attribute=otp"),
        "Personal/Key[work/admin]/Auth/Token?attribute=otp"
    );
}
