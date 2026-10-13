// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Projection of a decided image onto the dry-run plan. No Docker: the
//! decision is the input here, not something these tests resolve.

use super::*;
use crate::runtime::image::ImageInvalidationReason;

fn selector() -> RoleSelector {
    RoleSelector::parse("donbeave/the-architect").expect("role selector must parse")
}

#[test]
fn a_reused_image_reports_its_tag_and_no_reason() {
    let plan = plan_from_decision(
        &selector(),
        None,
        &ImageDecision::Reuse {
            image: "jk_the-architect:abc1234".to_owned(),
        },
        Some("projectjackin/the-architect:latest".to_owned()),
    );
    assert_eq!(plan.decision, "reuse");
    assert_eq!(plan.reason, None);
    assert_eq!(plan.image, "jk_the-architect:abc1234");
    assert_eq!(plan.base_image, None);
    assert_eq!(plan.role_git_sha.as_deref(), Some("abc1234"));
    assert_eq!(
        plan.published_image.as_deref(),
        Some("projectjackin/the-architect:latest")
    );
}

#[test]
fn a_background_refresh_reports_its_invalidation_reason() {
    let plan = plan_from_decision(
        &selector(),
        None,
        &ImageDecision::RefreshInBackground {
            image: "jk_the-architect:abc1234".to_owned(),
            reason: ImageInvalidationReason::AgentVersionChanged,
        },
        None,
    );
    assert_eq!(plan.decision, "refresh_in_background");
    assert_eq!(plan.reason, Some("agent_version_changed"));
    assert_eq!(plan.published_image, None);
}

#[test]
fn a_published_base_build_reports_the_base_it_starts_from() {
    let plan = plan_from_decision(
        &selector(),
        None,
        &ImageDecision::BuildFromPublished {
            reason: ImageInvalidationReason::RecipeHashChanged,
            role_git_sha: Some("deadbee".to_owned()),
            base_image: "projectjackin/the-architect:latest".to_owned(),
        },
        Some("projectjackin/the-architect:latest".to_owned()),
    );
    assert_eq!(plan.decision, "build_from_published");
    assert_eq!(plan.reason, Some("recipe_hash_changed"));
    assert_eq!(
        plan.base_image.as_deref(),
        Some("projectjackin/the-architect:latest")
    );
    assert!(
        plan.image.contains("deadbee"),
        "a build variant derives its tag from the role sha, got {}",
        plan.image
    );
}

#[test]
fn a_branch_build_derives_a_branch_scoped_tag() {
    let plan = plan_from_decision(
        &selector(),
        Some("feat/my-pr"),
        &ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::ExplicitRebuild,
            role_git_sha: Some("deadbee".to_owned()),
        },
        None,
    );
    assert_eq!(plan.decision, "build_from_workspace");
    assert_eq!(plan.reason, Some("explicit_rebuild"));
    assert_eq!(
        plan.image,
        image_name_for_branch(&selector(), "feat/my-pr", Some("deadbee")),
        "a branch build must not overwrite the stable image tag"
    );
}

#[test]
fn the_json_projection_carries_every_decision_field() {
    let plan = plan_from_decision(
        &selector(),
        None,
        &ImageDecision::BuildFromPublished {
            reason: ImageInvalidationReason::RoleGitShaChanged,
            role_git_sha: Some("deadbee".to_owned()),
            base_image: "projectjackin/the-architect:latest".to_owned(),
        },
        None,
    );
    let value = plan.to_json();
    for key in ["decision", "reason", "image", "base_image", "role_git_sha"] {
        assert!(
            value.get(key).is_some(),
            "the dry-run image_decision object must carry {key}, got {value}"
        );
    }
    assert_eq!(value["decision"], "build_from_published");
    assert_eq!(value["reason"], "role_git_sha_changed");
}
