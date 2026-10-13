// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `image_decision` — `ImageInvalidationReason`, `ImageDecision`,
//! and the label classifier (`classify_image_labels`).

use super::*;
use crate::image_recipe::{expected_image_recipe_for_test, image_recipe_label_map_for_test};
use crate::naming::{
    LABEL_IMAGE_CAPSULE_VERSION, LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_MANIFEST_VERSION,
    LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA,
};
use jackin_core::Agent;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_manifest::repo::CachedRepo;
use std::collections::HashMap;

fn seed_valid_role_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        jackin_manifest::repo_contract::BASE_DOCKERFILE_FROM,
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
}

fn validated_test_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
) -> (CachedRepo, jackin_manifest::repo::ValidatedRoleRepo) {
    let cached_repo = CachedRepo::new(paths, selector);
    seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    (cached_repo, validated_repo)
}

#[test]
fn image_label_classifier_reports_precise_invalidation_reasons() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let expected_hash = expected.hash.clone();

    let labels = HashMap::new();
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::MissingRecipeLabel)
    );

    let labels = [(LABEL_IMAGE_RECIPE_VERSION.to_owned(), "future".to_owned())].into();
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::RecipeVersionChanged)
    );

    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    labels.insert(LABEL_IMAGE_RECIPE_HASH.to_owned(), "old".to_owned());
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::RecipeHashChanged)
    );

    let labels = [
        (LABEL_IMAGE_RECIPE_VERSION.to_owned(), "v1".to_owned()),
        (LABEL_IMAGE_RECIPE_HASH.to_owned(), expected_hash.clone()),
    ]
    .into();
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::RecipeVersionChanged)
    );

    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    labels.insert(
        LABEL_IMAGE_CONSTRUCT.to_owned(),
        "projectjackin/old-construct:latest".to_owned(),
    );
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::ConstructImageChanged)
    );

    // Only the minimal kept labels report a precise, component-specific reason.
    // Every other recipe input now invalidates via the master recipe hash
    // (RecipeHashChanged) — see `recipe_diagnostic_labels`.
    for (label, reason) in [
        (
            LABEL_IMAGE_ROLE_GIT_SHA,
            ImageInvalidationReason::RoleGitShaChanged,
        ),
        (
            LABEL_IMAGE_MANIFEST_VERSION,
            ImageInvalidationReason::ManifestVersionChanged,
        ),
        (
            LABEL_IMAGE_CAPSULE_VERSION,
            ImageInvalidationReason::CapsuleVersionChanged,
        ),
    ] {
        let mut labels = image_recipe_label_map_for_test(
            &cached_repo,
            &validated_repo,
            Agent::Claude,
            Some("abc123"),
            None,
            None,
            "0",
        );
        labels.insert(label.to_owned(), "stale".to_owned());
        let expected = expected_image_recipe_for_test(
            &cached_repo,
            &validated_repo,
            Agent::Claude,
            Some("abc123"),
            None,
            None,
            "0",
        );
        assert_eq!(
            classify_image_labels(&labels, &[expected]),
            Some(reason),
            "{label} mismatch should report the precise invalidation reason"
        );
    }
}
