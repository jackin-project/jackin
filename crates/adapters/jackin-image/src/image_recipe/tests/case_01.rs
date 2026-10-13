// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn image_recipe_canonicalizes_supported_agent_order() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    seed_valid_role_repo(&cached_repo.repo_dir);
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
        &crate::naming::image_name(&selector, None),
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
        &crate::naming::image_name(&selector, None),
    )
    .unwrap();

    // Labels written while launching Claude satisfy the recipe expected when
    // launching Codex — one image, reused across agents.
    assert_eq!(classify_image_labels(&labels_claude, &expected_codex), None);
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
        Some(ClassificationReason::MissingRecipeLabel)
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
        Some(ClassificationReason::RecipeVersionChanged)
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
        Some(ClassificationReason::RecipeHashChanged)
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
        Some(ClassificationReason::RecipeVersionChanged)
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
        Some(ClassificationReason::ConstructImageChanged)
    );

    // Only the minimal kept labels report a precise, component-specific reason.
    // Every other recipe input now invalidates via the master recipe hash
    // (RecipeHashChanged) — see `recipe_diagnostic_label_keys`.
    for (label, reason) in [
        (
            LABEL_IMAGE_ROLE_GIT_SHA,
            ClassificationReason::RoleGitShaChanged,
        ),
        (
            LABEL_IMAGE_MANIFEST_VERSION,
            ClassificationReason::ManifestVersionChanged,
        ),
        (
            LABEL_IMAGE_CAPSULE_VERSION,
            ClassificationReason::CapsuleVersionChanged,
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
