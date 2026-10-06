// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `op_breadcrumb`.
use super::parse_path_breadcrumb;

#[test]
fn parse_path_breadcrumb_3_segment_no_subtitle() {
    let p = parse_path_breadcrumb("Private/Stripe/api key").unwrap();
    assert_eq!(p.vault, "Private");
    assert_eq!(p.item, "Stripe");
    assert!(p.item_subtitle.is_none());
    assert!(p.section.is_none());
    assert_eq!(p.field, "api key");
    assert!(p.attribute_query.is_none());
}

#[test]
fn parse_path_breadcrumb_3_segment_with_subtitle() {
    let p = parse_path_breadcrumb("Private/Claude[alexey@zhokhov.com]/auth").unwrap();
    assert_eq!(p.vault, "Private");
    assert_eq!(p.item, "Claude");
    assert_eq!(p.item_subtitle.as_deref(), Some("alexey@zhokhov.com"));
    assert!(p.section.is_none());
    assert_eq!(p.field, "auth");
}

#[test]
fn parse_path_breadcrumb_4_segment_with_subtitle() {
    let p =
        parse_path_breadcrumb("Private/Claude[alexey@zhokhov.com]/security/auth token").unwrap();
    assert_eq!(p.vault, "Private");
    assert_eq!(p.item, "Claude");
    assert_eq!(p.item_subtitle.as_deref(), Some("alexey@zhokhov.com"));
    assert_eq!(p.section.as_deref(), Some("security"));
    assert_eq!(p.field, "auth token");
}

#[test]
fn parse_path_breadcrumb_with_attribute_query() {
    let p = parse_path_breadcrumb("Private/GitHub/one-time password?attribute=otp").unwrap();
    assert_eq!(p.field, "one-time password");
    assert_eq!(p.attribute_query.as_deref(), Some("?attribute=otp"));
}

#[test]
fn parse_path_breadcrumb_subtitle_containing_brackets() {
    let p = parse_path_breadcrumb("Private/Claude[has [bracket]]/auth").unwrap();
    assert_eq!(p.item, "Claude[has ");
    assert_eq!(p.item_subtitle.as_deref(), Some("bracket]"));
}

#[test]
fn parse_path_breadcrumb_invalid_too_few_segments() {
    assert!(parse_path_breadcrumb("Private/Item").is_none());
    assert!(parse_path_breadcrumb("Private").is_none());
    assert!(parse_path_breadcrumb("").is_none());
}

#[test]
fn parse_path_breadcrumb_invalid_too_many_segments() {
    assert!(parse_path_breadcrumb("a/b/c/d/e").is_none());
}
