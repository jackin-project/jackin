// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn custom_construct_identity_changes_recipe_hash() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let canonical = build_image_recipe_with_construct_image(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        "0",
        jackin_manifest::repo_contract::CONSTRUCT_IMAGE.to_owned(),
    )
    .unwrap();
    let custom = build_image_recipe_with_construct_image(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        "0",
        "localhost/projectjackin-construct:test".to_owned(),
    )
    .unwrap();

    assert_ne!(
        canonical.hash().unwrap(),
        custom.hash().unwrap(),
        "construct image identity must participate in the recipe hash"
    );
}
