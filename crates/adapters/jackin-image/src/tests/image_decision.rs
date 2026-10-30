// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::image_decision::{
    ImageDecision, ImageInvalidationReason, build_decision, classify_image_labels,
};

#[test]
fn invalidation_reason_spellings_match_wire_values() {
    assert_eq!(
        ImageInvalidationReason::ExplicitRebuild.as_str(),
        "explicit_rebuild"
    );
    assert_eq!(
        ImageInvalidationReason::RecipeHashChanged.as_str(),
        "recipe_hash_changed"
    );
    assert_eq!(
        ImageInvalidationReason::AgentVersionChanged.as_str(),
        "agent_version_changed"
    );
}

#[test]
fn build_decision_without_override_builds_from_workspace() {
    let decision = build_decision(
        ImageInvalidationReason::RecipeHashChanged,
        Some("abcdef1".to_owned()),
        None,
    );
    assert_eq!(decision.kind(), "build_from_workspace");
    assert_eq!(
        decision.reason(),
        Some(ImageInvalidationReason::RecipeHashChanged)
    );
    assert_eq!(decision.role_git_sha(), Some("abcdef1".to_owned()));
    assert_eq!(decision.resolved_image(), None);
    assert_eq!(decision.base_image_ref(), None);
}

#[test]
fn build_decision_with_override_builds_from_published_base() {
    let decision = build_decision(
        ImageInvalidationReason::PublishedImageStale,
        Some("abcdef1".to_owned()),
        Some("registry.example.com/base:trixie"),
    );
    assert_eq!(decision.kind(), "build_from_published");
    assert_eq!(
        decision.base_image_ref(),
        Some("registry.example.com/base:trixie")
    );
    assert_eq!(decision.resolved_image(), None);
}

#[test]
fn reuse_decision_exposes_resolved_image_and_no_reason() {
    let decision = ImageDecision::Reuse {
        image: "jk_myrole:abcdef1".to_owned(),
    };
    assert_eq!(decision.kind(), "reuse");
    assert_eq!(decision.reason(), None);
    assert_eq!(decision.resolved_image(), Some("jk_myrole:abcdef1"));
    assert_eq!(decision.role_git_sha(), Some("abcdef1".to_owned()));
}

#[test]
fn refresh_decision_exposes_image_and_reason() {
    let decision = ImageDecision::RefreshInBackground {
        image: "jk_myrole:abcdef1".to_owned(),
        reason: ImageInvalidationReason::CapsuleVersionChanged,
    };
    assert_eq!(decision.kind(), "refresh_in_background");
    assert_eq!(
        decision.reason(),
        Some(ImageInvalidationReason::CapsuleVersionChanged)
    );
    assert_eq!(decision.resolved_image(), Some("jk_myrole:abcdef1"));
}

#[test]
fn classify_labels_without_recipe_version_reports_missing() {
    let reason = classify_image_labels(&HashMap::new(), &[]);
    assert_eq!(reason, Some(ImageInvalidationReason::MissingRecipeLabel));
}

#[test]
fn classify_labels_with_wrong_recipe_version_reports_changed() {
    let labels = HashMap::from([(
        crate::naming::LABEL_IMAGE_RECIPE_VERSION.to_owned(),
        "v0".to_owned(),
    )]);
    let reason = classify_image_labels(&labels, &[]);
    assert_eq!(reason, Some(ImageInvalidationReason::RecipeVersionChanged));
}
