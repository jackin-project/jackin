// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::runtime::naming::{image_name, image_name_for_branch, role_base_image_name};
use jackin_core::RoleSelector;
use jackin_manifest::repo::CachedRepo;

#[test]
fn reuse_staleness_sentinel_gate_uses_published_image_or_stored_agent_version() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (_cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    assert!(
        !reuse_needs_background_staleness_check(&paths, &validated_repo, "jk_agent-smith"),
        "roles without published images or stored versions should not spawn the sentinel"
    );

    version_check::store_version(&paths, Agent::Claude, "jk_agent-smith", "1.2.3");
    assert!(
        reuse_needs_background_staleness_check(&paths, &validated_repo, "jk_agent-smith"),
        "stored agent-version baselines should spawn the sentinel"
    );

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
    let validated_with_published =
        jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    assert!(
        reuse_needs_background_staleness_check(&paths, &validated_with_published, "other-image"),
        "declared published images should spawn the sentinel even without stored versions"
    );
}

#[tokio::test]
async fn hook_content_change_invalidates_image_recipe() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::create_dir_all(cached_repo.repo_dir.join("hooks")).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("hooks/preflight.sh"),
        "echo old\n",
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
preflight = "hooks/preflight.sh"
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    std::fs::write(
        cached_repo.repo_dir.join("hooks/preflight.sh"),
        "echo new\n",
    )
    .unwrap();

    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, None)]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
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
            reason: ImageInvalidationReason::RecipeHashChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
}

#[tokio::test]
async fn branch_override_uses_branch_tag_and_recipe_ref() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let branch = "feat/instant-launch";
    // The runner returns HEAD SHA `abc123`, so the reuse lookup keys on the
    // commit-tagged branch image (`jk_agent-smith_feat-instant-launch:abc123`).
    let image = image_name_for_branch(&selector, branch, Some("abc123"));
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let local_base = role_base_image_name(&selector, Some(branch), Some("abc123"));
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        Some(branch),
        Some(local_base.as_str()),
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        Some(branch),
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(decision, ImageDecision::Reuse { image });
}

#[tokio::test]
async fn decide_agent_image_rebuilds_when_role_git_sha_has_changed() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    labels.insert(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "old-sha".to_owned());
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, None)]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
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
            reason: ImageInvalidationReason::RoleGitShaChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
}

#[tokio::test]
async fn decide_agent_image_rebuilds_when_role_source_ref_has_changed() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        Some("main"),
        None,
        "0",
    );
    let image = image_name_for_branch(&selector, "feature/instant-launch", None);
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        Some("feature/instant-launch"),
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::RecipeHashChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
}

#[tokio::test]
async fn decide_agent_image_reuses_when_host_uid_matches_recipe() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let local_base = role_base_image_name(&selector, None, Some("abc123"));
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        Some(local_base.as_str()),
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, Some("abc123"))]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
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
        ImageDecision::Reuse {
            image: image_name(&selector, Some("abc123")),
        }
    );
}

#[tokio::test]
async fn decide_agent_image_rebuilds_when_cached_recipe_uses_non_local_base() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, Some("abc123"))]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
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
            reason: ImageInvalidationReason::RecipeHashChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
}

#[test]
fn host_uid_changes_recipe_hash() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let mut first = build_image_recipe(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        "0",
    )
    .unwrap();
    let mut second = first.clone();
    first.host_uid = Some(501);
    second.host_uid = Some(1000);

    assert_ne!(
        first.hash().unwrap(),
        second.hash().unwrap(),
        "host UID must participate in the derived image recipe hash"
    );
}
