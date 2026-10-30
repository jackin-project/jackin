// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::RoleSelector;

use crate::naming::{image_name, image_name_for_branch, role_base_image_name, short_git_sha};

fn selector() -> RoleSelector {
    RoleSelector::parse("myrole").expect("fixture selector parses")
}

#[test]
fn short_git_sha_truncates_to_seven_characters() {
    assert_eq!(short_git_sha("abcdef1234567890"), "abcdef1");
}

#[test]
fn short_git_sha_keeps_short_input_unchanged() {
    assert_eq!(short_git_sha("abc"), "abc");
}

#[test]
fn image_name_without_sha_has_no_tag() {
    assert_eq!(image_name(&selector(), None), "jk_myrole");
}

#[test]
fn image_name_with_sha_tags_short_prefix() {
    assert_eq!(
        image_name(&selector(), Some("abcdef1234567890")),
        "jk_myrole:abcdef1"
    );
}

#[test]
fn image_name_ignores_non_hex_sha() {
    assert_eq!(image_name(&selector(), Some("zzzzzzzz")), "jk_myrole");
}

#[test]
fn image_name_ignores_empty_sha() {
    assert_eq!(image_name(&selector(), Some("")), "jk_myrole");
}

#[test]
fn image_name_for_branch_slugifies_branch() {
    assert_eq!(
        image_name_for_branch(&selector(), "Feature/Add", None),
        "jk_myrole_feature-add"
    );
}

#[test]
fn image_name_for_branch_tags_sha() {
    assert_eq!(
        image_name_for_branch(&selector(), "main", Some("abcdef1234567890")),
        "jk_myrole_main:abcdef1"
    );
}

#[test]
fn role_base_image_name_without_branch() {
    assert_eq!(
        role_base_image_name(&selector(), None, None),
        "jk_myrole__base"
    );
}

#[test]
fn role_base_image_name_with_branch_and_sha() {
    assert_eq!(
        role_base_image_name(&selector(), Some("Main"), Some("abcdef1234567890")),
        "jk_myrole_main__base:abcdef1"
    );
}
