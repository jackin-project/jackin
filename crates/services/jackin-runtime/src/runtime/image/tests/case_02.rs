// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parse_buildkit_duration_ms_handles_fraction_shapes() {
    assert_eq!(parse_buildkit_duration_ms("9s"), Some(9000));
    assert_eq!(parse_buildkit_duration_ms("9.2s"), Some(9200));
    assert_eq!(parse_buildkit_duration_ms("9.23s"), Some(9230));
    assert_eq!(parse_buildkit_duration_ms("9.234s"), Some(9234));
    assert_eq!(parse_buildkit_duration_ms("9.2349s"), Some(9234));
    assert_eq!(parse_buildkit_duration_ms("9ms"), None);
    assert_eq!(parse_buildkit_duration_ms("abc"), None);
}

#[test]
fn compact_image_warning_line_is_not_debug_prefixed() {
    let line = compact_image_warning_line("docker pull image failed");
    assert_eq!(line, "jackin: warning: docker pull image failed");
    assert!(!line.contains("[jackin debug"));
}

#[tokio::test]
async fn published_image_fresh_when_sha_matches() {
    let docker = make_docker([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "abc123".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(!stale, "matching SHA should report image as fresh");
}

#[tokio::test]
async fn published_image_stale_when_sha_differs() {
    let docker = make_docker([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "oldsha".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("newsha"), &docker).await;
    assert!(stale, "mismatched SHA should report image as stale");
}

#[tokio::test]
async fn published_image_stale_when_sha_label_missing_and_sha_known() {
    let docker = make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(), "0.1".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(stale, "known role SHA requires a matching SHA label");
}

#[tokio::test]
async fn published_image_falls_back_to_construct_version_when_sha_unknown() {
    let docker = make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(), "0.1".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", None, &docker).await;
    assert!(
        !stale,
        "matching construct version should be fresh before role SHA is known"
    );
}

#[tokio::test]
async fn published_image_stale_when_construct_version_differs() {
    let docker = make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(), "0.0".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(
        stale,
        "outdated construct version should report image as stale"
    );
}

#[tokio::test]
async fn published_image_stale_when_no_labels_and_sha_known() {
    let docker = make_docker(HashMap::new());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(stale, "known role SHA requires a matching SHA label");
}

#[tokio::test]
async fn published_image_stale_when_pull_fails() {
    let docker = FakeDockerClient {
        fail_with: vec![("docker pull".to_owned(), "network error".to_owned())],
        ..FakeDockerClient::default()
    };
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(stale, "pull failure should report image as stale");
}

#[tokio::test]
async fn published_image_stale_when_inspect_image_labels_fails() {
    let docker = FakeDockerClient {
        fail_with: vec![(
            "docker inspect image:".to_owned(),
            "daemon error".to_owned(),
        )],
        ..FakeDockerClient::default()
    };
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc"), &docker).await;
    assert!(
        stale,
        "inspect_image_labels failure should treat image as stale"
    );
}

#[test]
fn local_role_base_reuse_accepts_sha_only_published_labels() {
    let labels = HashMap::from([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "abc123".to_owned())]);
    assert!(local_role_base_labels_match(
        &labels,
        "projectjackin/construct:trixie",
        "0.1-trixie",
        Some("abc123"),
    ));
}

#[test]
fn local_role_base_reuse_rejects_stale_construct_label() {
    let labels = HashMap::from([
        (LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "abc123".to_owned()),
        (
            LABEL_IMAGE_CONSTRUCT.to_owned(),
            "projectjackin/construct:old".to_owned(),
        ),
    ]);
    assert!(!local_role_base_labels_match(
        &labels,
        "projectjackin/construct:trixie",
        "0.1-trixie",
        Some("abc123"),
    ));
}

#[tokio::test]
async fn published_stale_role_base_build_keeps_layer_cache() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    let base = ensure_local_role_base(
        &selector,
        None,
        Some("abc123"),
        &cached_repo,
        &validated_repo,
        None,
        false,
        false,
        &docker,
        &mut runner,
        None,
    )
    .await
    .unwrap();

    assert_eq!(base, role_base_image_name(&selector, None, Some("abc123")));
    let build = recorded_docker_build(&runner);
    assert!(
        !build.contains(" --pull "),
        "published-stale role-base builds should preserve Docker layer cache: {build}"
    );
}

#[tokio::test]
async fn explicit_rebuild_role_base_still_pulls_default_construct() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    ensure_local_role_base(
        &selector,
        None,
        Some("abc123"),
        &cached_repo,
        &validated_repo,
        None,
        true,
        false,
        &docker,
        &mut runner,
        None,
    )
    .await
    .unwrap();

    let build = recorded_docker_build(&runner);
    assert!(
        build.contains(" --pull "),
        "explicit rebuild should keep full base refresh semantics: {build}"
    );
}

#[test]
fn cache_bust_policy_preserves_stored_value_for_published_stale() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (_, validated_repo) = validated_test_repo(&paths, &selector);
    let image = image_name(&selector, Some("abc123"));
    version_check::store_cache_bust(&paths, &image, "stored-bust");

    let mint = should_mint_fresh_cache_bust(false, ImageInvalidationReason::PublishedImageStale);
    let value = cache_bust_value_for_build(&paths, &image, &validated_repo.manifest, mint).unwrap();

    assert!(!mint);
    assert_eq!(value, "stored-bust");
    assert_eq!(
        version_check::stored_cache_bust(&paths, &image).as_deref(),
        Some("stored-bust")
    );
}

#[test]
fn cache_bust_policy_mints_for_explicit_rebuild() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (_, validated_repo) = validated_test_repo(&paths, &selector);
    let image = image_name(&selector, Some("abc123"));
    version_check::store_cache_bust(&paths, &image, "stored-bust");

    let mint = should_mint_fresh_cache_bust(true, ImageInvalidationReason::ExplicitRebuild);
    let value = cache_bust_value_for_build(&paths, &image, &validated_repo.manifest, mint).unwrap();

    assert!(mint);
    assert_ne!(value, "stored-bust");
    assert_eq!(
        version_check::stored_cache_bust(&paths, &image).as_deref(),
        Some(value.as_str())
    );
}

#[test]
fn cache_bust_policy_mints_for_agent_version_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (_, validated_repo) = validated_test_repo(&paths, &selector);
    let image = image_name(&selector, Some("abc123"));
    version_check::store_cache_bust(&paths, &image, "stored-bust");

    let mint = should_mint_fresh_cache_bust(false, ImageInvalidationReason::AgentVersionChanged);
    let value = cache_bust_value_for_build(&paths, &image, &validated_repo.manifest, mint).unwrap();

    assert!(mint);
    assert_ne!(value, "stored-bust");
    assert_eq!(
        version_check::stored_cache_bust(&paths, &image).as_deref(),
        Some(value.as_str())
    );
}

#[test]
fn image_recipe_canonicalizes_supported_agent_order() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let claude_first = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let claude_first_labels = image_recipe_label_map_for_test(
        &cached_repo,
        &claude_first,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );

    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["kimi", "claude"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let kimi_first = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let kimi_first_labels = image_recipe_label_map_for_test(
        &cached_repo,
        &kimi_first,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );

    assert_eq!(
        claude_first_labels.get(LABEL_IMAGE_RECIPE_HASH),
        kimi_first_labels.get(LABEL_IMAGE_RECIPE_HASH),
        "recipe hash should be stable for same supported-agent set"
    );
}

#[test]
fn image_recipe_accepts_script_fallback_install_recipe() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_install_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
        AgentInstall::ScriptFallback,
    );
    let expected = expected_image_recipes(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        &paths,
        &image_name(&selector, None),
    )
    .unwrap();

    assert_eq!(classify_image_labels(&labels, &expected), None);
}

#[test]
fn image_recipe_is_agent_independent() {
    // The recipe (and thus the image identity) keys on the supported-agent set,
    // never the selected agent — so the same role yields one recipe hash
    // regardless of which agent is launched. Selecting a different initial agent
    // must reuse the warm image instead of forking a redundant one.
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);

    let labels_claude = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let expected_codex = expected_image_recipes(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        &paths,
        &image_name(&selector, None),
    )
    .unwrap();

    // Labels written while launching Claude satisfy the recipe expected when
    // launching Codex — one image, reused across agents.
    assert_eq!(classify_image_labels(&labels_claude, &expected_codex), None);
}
