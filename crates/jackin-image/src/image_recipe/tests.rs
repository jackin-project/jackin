//! Tests for `image_recipe` — recipe value type, label generation,
//! and the diagnostic-label classifier (in concert with the runtime
//! `ImageInvalidationReason` set).

use super::*;
use crate::derived_image::AgentInstall;
use jackin_core::agent::Agent;
use jackin_core::selector::RoleSelector;
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

/// Local mirror of the runtime's `classify_image_labels` so recipe tests
/// can validate label mismatch semantics without depending on the
/// decision-type crate (D1 PART F). Behavioural contract is identical:
/// `Some(ImageInvalidationReason)` when the stored labels do not match
/// any current recipe; `None` when at least one expected recipe hash
/// matches and the diagnostic labels agree.
fn classify_image_labels(
    labels: &HashMap<String, String>,
    expected_recipes: &[ExpectedImageRecipe],
) -> Option<ClassificationReason> {
    match labels.get(LABEL_IMAGE_RECIPE_VERSION).map(String::as_str) {
        Some(IMAGE_RECIPE_VERSION) => {}
        Some(_) => return Some(ClassificationReason::RecipeVersionChanged),
        None => return Some(ClassificationReason::MissingRecipeLabel),
    }
    let Some(stored_hash) = labels.get(LABEL_IMAGE_RECIPE_HASH) else {
        return Some(ClassificationReason::MissingRecipeLabel);
    };

    for expected in expected_recipes {
        if &expected.hash == stored_hash {
            return recipe_label_mismatch(labels, &expected.recipe);
        }
    }

    let Some(first_expected) = expected_recipes.first() else {
        return Some(ClassificationReason::RecipeHashChanged);
    };
    recipe_label_mismatch(labels, &first_expected.recipe)
        .or(Some(ClassificationReason::RecipeHashChanged))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassificationReason {
    MissingRecipeLabel,
    RecipeVersionChanged,
    RecipeHashChanged,
    RoleGitShaChanged,
    ManifestVersionChanged,
    ConstructImageChanged,
    CapsuleVersionChanged,
}

fn recipe_label_mismatch(
    labels: &HashMap<String, String>,
    recipe: &ImageRecipe,
) -> Option<ClassificationReason> {
    for (key, expected) in recipe.recipe_diagnostic_label_keys() {
        let Some(stored) = labels.get(key) else {
            return Some(ClassificationReason::MissingRecipeLabel);
        };
        if stored != &expected {
            return Some(match key {
                LABEL_IMAGE_ROLE_GIT_SHA => ClassificationReason::RoleGitShaChanged,
                LABEL_IMAGE_MANIFEST_VERSION => ClassificationReason::ManifestVersionChanged,
                LABEL_IMAGE_CONSTRUCT => ClassificationReason::ConstructImageChanged,
                LABEL_IMAGE_CAPSULE_VERSION => ClassificationReason::CapsuleVersionChanged,
                _ => ClassificationReason::RecipeHashChanged,
            });
        }
    }
    None
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
