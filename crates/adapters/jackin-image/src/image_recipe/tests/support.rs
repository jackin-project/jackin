// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn seed_valid_role_repo(repo_dir: &std::path::Path) {
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

pub(super) fn validated_test_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
) -> (CachedRepo, jackin_manifest::repo::ValidatedRoleRepo) {
    let cached_repo = CachedRepo::new(paths, selector);
    seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    (cached_repo, validated_repo)
}

pub(super) fn classify_image_labels(
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
pub(super) enum ClassificationReason {
    MissingRecipeLabel,
    RecipeVersionChanged,
    RecipeHashChanged,
    RoleGitShaChanged,
    ManifestVersionChanged,
    ConstructImageChanged,
    CapsuleVersionChanged,
}

pub(super) fn recipe_label_mismatch(
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
