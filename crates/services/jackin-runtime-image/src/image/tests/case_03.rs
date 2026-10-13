// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::RoleSelector;
use jackin_manifest::repo::CachedRepo;
use jackin_runtime_naming::naming::{image_name, role_base_image_name};

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

#[tokio::test]
async fn decide_agent_image_builds_from_published_when_declared_image_is_missing() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromPublished {
            reason: ImageInvalidationReason::LocalImageMissing,
            role_git_sha: None,
            base_image: "docker.io/myorg/my-role:latest".to_owned(),
        }
    );
    // HEAD SHA is resolved up front (it is the image tag); the only command on
    // the missing-image path is that git capture.
    assert!(
        runner.recorded.iter().all(|c| c.contains("rev-parse HEAD")),
        "missing-image path should run only the role-SHA git capture; got: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn decide_agent_image_builds_from_workspace_when_published_image_is_stale() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let docker = FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            "old-sha".to_owned(),
        )]));
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::PublishedImageStale,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
        "published image freshness must be checked before binary prep: {recorded:?}"
    );
}

#[tokio::test]
async fn decide_agent_image_build_path_checks_published_image_after_recipe_mismatch() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = image_name(&selector, Some("abc123"));
    let mut stale_local_labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        Some(role_base_image_name(&selector, None, Some("abc123")).as_str()),
        "0",
    );
    stale_local_labels.insert(LABEL_IMAGE_RECIPE_HASH.to_owned(), "old-recipe".to_owned());

    for (published_labels, expected_base) in [
        (
            HashMap::from([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "abc123".to_owned())]),
            Some("docker.io/myorg/my-role:latest".to_owned()),
        ),
        (
            HashMap::from([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "old-sha".to_owned())]),
            None,
        ),
    ] {
        let docker = FakeDockerClient::default();
        docker
            .list_image_tags_queue
            .borrow_mut()
            .push_back(vec![image.clone()]);
        docker
            .inspect_image_labels_queue
            .borrow_mut()
            .push_back(stale_local_labels.clone());
        docker
            .inspect_image_labels_queue
            .borrow_mut()
            .push_back(published_labels);
        let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

        let decision = decide_role_image(
            &paths,
            &selector,
            &cached_repo,
            &validated_repo,
            false,
            None,
            None,
            &docker,
            &mut runner,
        )
        .await
        .unwrap();

        match (decision, expected_base) {
            (
                ImageDecision::BuildFromPublished {
                    reason,
                    role_git_sha,
                    base_image,
                },
                Some(expected_base),
            ) => {
                assert_eq!(reason, ImageInvalidationReason::RecipeHashChanged);
                assert_eq!(role_git_sha.as_deref(), Some("abc123"));
                assert_eq!(base_image, expected_base);
            }
            (
                ImageDecision::BuildFromWorkspace {
                    reason,
                    role_git_sha,
                },
                None,
            ) => {
                assert_eq!(reason, ImageInvalidationReason::RecipeHashChanged);
                assert_eq!(role_git_sha.as_deref(), Some("abc123"));
            }
            (other, expected) => {
                panic!("unexpected decision {other:?} for expected base {expected:?}");
            }
        }
        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
            "build path must check published image freshness: {recorded:?}"
        );
    }
}
