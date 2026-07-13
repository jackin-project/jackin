// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ImageInvalidationReason` + `ImageDecision` value types and the label-classifier helpers.
//!
//! Architecture Invariant: L1 application crate module. Depends on `jackin-core`,
//! `jackin-image` siblings (`image_recipe`).

use std::collections::HashMap;

use crate::image_recipe::IMAGE_RECIPE_VERSION;
use crate::naming::{LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageInvalidationReason {
    ExplicitRebuild,
    LocalImageMissing,
    ImageListFailed,
    MissingRecipeLabel,
    RecipeVersionChanged,
    RecipeHashChanged,
    RoleGitShaChanged,
    ManifestVersionChanged,
    ConstructImageChanged,
    CapsuleVersionChanged,
    PublishedImageStale,
    InspectFailed,
    /// D20: an agent CLI release is newer than the version baked into the image.
    AgentVersionChanged,
}

impl ImageInvalidationReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitRebuild => "explicit_rebuild",
            Self::LocalImageMissing => "local_image_missing",
            Self::ImageListFailed => "image_list_failed",
            Self::MissingRecipeLabel => "missing_recipe_label",
            Self::RecipeVersionChanged => "recipe_version_changed",
            Self::RecipeHashChanged => "recipe_hash_changed",
            Self::RoleGitShaChanged => "role_git_sha_changed",
            Self::ManifestVersionChanged => "manifest_version_changed",
            Self::ConstructImageChanged => "construct_image_changed",
            Self::CapsuleVersionChanged => "capsule_version_changed",
            Self::PublishedImageStale => "published_image_stale",
            Self::InspectFailed => "inspect_failed",
            Self::AgentVersionChanged => "agent_version_changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageDecision {
    Reuse {
        image: String,
    },
    RefreshInBackground {
        image: String,
        reason: ImageInvalidationReason,
    },
    BuildFromPublished {
        reason: ImageInvalidationReason,
        role_git_sha: Option<String>,
        base_image: String,
    },
    BuildFromWorkspace {
        reason: ImageInvalidationReason,
        role_git_sha: Option<String>,
    },
}

impl ImageDecision {
    /// Role-repo commit SHA embedded in this decision — short SHA from the
    /// image tag for Reuse/Refresh, or the value from Build* variants.
    pub fn role_git_sha(&self) -> Option<String> {
        match self {
            Self::Reuse { image } | Self::RefreshInBackground { image, .. } => {
                image.split(':').next_back().map(ToOwned::to_owned)
            }
            Self::BuildFromWorkspace { role_git_sha, .. }
            | Self::BuildFromPublished { role_git_sha, .. } => role_git_sha.clone(),
        }
    }

    /// Base/construct image reference for this decision; `None` for Reuse/Refresh.
    pub fn base_image_ref(&self) -> Option<&str> {
        match self {
            Self::BuildFromPublished { base_image, .. } => Some(base_image.as_str()),
            _ => None,
        }
    }
}

pub fn build_decision(
    reason: ImageInvalidationReason,
    role_git_sha: Option<String>,
    base_image_override: Option<&str>,
) -> ImageDecision {
    match base_image_override {
        Some(base_image) => ImageDecision::BuildFromPublished {
            reason,
            role_git_sha,
            base_image: base_image.to_owned(),
        },
        None => ImageDecision::BuildFromWorkspace {
            reason,
            role_git_sha,
        },
    }
}

pub fn decision_base_image_override<'a>(
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    branch_override: Option<&str>,
) -> Option<&'a str> {
    let custom_construct = jackin_manifest::repo_contract::construct_image()
        != jackin_manifest::repo_contract::CONSTRUCT_IMAGE;
    if branch_override.is_none() && !custom_construct {
        validated_repo.manifest.published_image.as_deref()
    } else {
        None
    }
}

pub fn classify_image_labels(
    labels: &HashMap<String, String>,
    expected_recipes: &[crate::image_recipe::ExpectedImageRecipe],
) -> Option<ImageInvalidationReason> {
    match labels.get(LABEL_IMAGE_RECIPE_VERSION).map(String::as_str) {
        Some(IMAGE_RECIPE_VERSION) => {}
        Some(_) => return Some(ImageInvalidationReason::RecipeVersionChanged),
        None => return Some(ImageInvalidationReason::MissingRecipeLabel),
    }
    let Some(stored_hash) = labels.get(LABEL_IMAGE_RECIPE_HASH) else {
        return Some(ImageInvalidationReason::MissingRecipeLabel);
    };

    for expected in expected_recipes {
        if &expected.hash == stored_hash {
            return recipe_label_mismatch(labels, &expected.recipe);
        }
    }

    let Some(first_expected) = expected_recipes.first() else {
        return Some(ImageInvalidationReason::RecipeHashChanged);
    };
    recipe_label_mismatch(labels, &first_expected.recipe)
        .or(Some(ImageInvalidationReason::RecipeHashChanged))
}

pub fn recipe_label_mismatch(
    labels: &HashMap<String, String>,
    recipe: &crate::image_recipe::ImageRecipe,
) -> Option<ImageInvalidationReason> {
    for (key, expected) in recipe.recipe_diagnostic_label_keys() {
        let Some(stored) = labels.get(key) else {
            return Some(ImageInvalidationReason::MissingRecipeLabel);
        };
        if stored != &expected {
            return Some(match key {
                crate::naming::LABEL_IMAGE_ROLE_GIT_SHA => {
                    ImageInvalidationReason::RoleGitShaChanged
                }
                crate::naming::LABEL_IMAGE_MANIFEST_VERSION => {
                    ImageInvalidationReason::ManifestVersionChanged
                }
                crate::naming::LABEL_IMAGE_CONSTRUCT => {
                    ImageInvalidationReason::ConstructImageChanged
                }
                crate::naming::LABEL_IMAGE_CAPSULE_VERSION => {
                    ImageInvalidationReason::CapsuleVersionChanged
                }
                _ => ImageInvalidationReason::RecipeHashChanged,
            });
        }
    }
    None
}

pub fn emit_image_decision(image: &str, reason: ImageInvalidationReason) {
    jackin_diagnostics::debug_log!(
        "image",
        "derived image {image} requires build: {}",
        reason.as_str()
    );
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "image_cache_miss",
            "derived image",
            &format!("derived image {image} requires build"),
            Some(reason.as_str()),
        );
    }
}

pub fn emit_image_reuse(image: &str) {
    if let Some(run) = jackin_diagnostics::active_run() {
        let detail = serde_json::json!({
            "reason": "recipe_hash_match",
            "skipped": [
                "prepare_runtime_binaries",
                "create_derived_build_context",
                "resolve_github_token",
                "docker_build",
                "selected_agent_version_probe",
                "published_image_pull",
                "agent_version_check"
            ],
        })
        .to_string();
        run.stage(
            "image_cache_hit",
            "derived image",
            &format!("reusing derived image {image}"),
            Some(&detail),
        );
    }
}

pub fn emit_image_refresh_background(image: &str, reason: ImageInvalidationReason) {
    emit_image_reuse(image);
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "image_refresh_background",
            "derived image",
            &format!("reusing derived image {image}; background refresh pending"),
            Some(reason.as_str()),
        );
    }
}

#[cfg(test)]
mod tests;
