// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `naming`.
use super::*;

#[test]
fn image_name_distinguishes_namespaced_and_flat_classes() {
    let namespaced = RoleSelector::new(Some("chainargos"), "agent-brown");
    let flat = RoleSelector::new(None, "chainargos-agent-brown");
    assert_ne!(image_name(&namespaced, None), image_name(&flat, None));
    assert_eq!(image_name(&namespaced, None), "jk_chainargos_agent-brown");
    assert_eq!(image_name(&flat, None), "jk_chainargos-agent-brown");
}

#[test]
fn image_name_flat_role_uses_jk_underscore_prefix() {
    let flat = RoleSelector::new(None, "agent-smith");
    assert_eq!(image_name(&flat, None), "jk_agent-smith");
}

#[test]
fn image_name_for_branch_substitutes_slashes_and_keeps_prefix() {
    let namespaced = RoleSelector::new(Some("chainargos"), "agent-brown");
    let flat = RoleSelector::new(None, "the-architect");

    assert_eq!(
        image_name_for_branch(&namespaced, "feat/my-pr", None),
        "jk_chainargos_agent-brown_feat-my-pr"
    );
    assert_eq!(
        image_name_for_branch(&flat, "main", None),
        "jk_the-architect_main"
    );
    // Branch with multiple slashes — all become dashes.
    assert_eq!(
        image_name_for_branch(&flat, "feat/scope/detail", None),
        "jk_the-architect_feat-scope-detail"
    );
}

#[test]
fn image_name_tags_with_short_role_commit_sha() {
    let flat = RoleSelector::new(None, "the-architect");
    // A role-repo commit SHA is rendered as the short (7-char) tag, matching the
    // `4f38b4f` form GitHub shows — never a mutable `:latest`.
    assert_eq!(
        image_name(&flat, Some("4f38b4f02b6a82e26c751d8adea3acc776c6d3b8")),
        "jk_the-architect:4f38b4f"
    );
    assert_eq!(
        image_name_for_branch(&flat, "feat/x", Some("4f38b4f02b6a")),
        "jk_the-architect_feat-x:4f38b4f"
    );
    // An empty SHA (a role checkout with no commits) leaves the bare name so
    // Docker defaults it to `:latest` rather than producing `name:`.
    assert_eq!(image_name(&flat, Some("")), "jk_the-architect");
    assert_eq!(image_name(&flat, None), "jk_the-architect");
}

#[test]
fn short_git_sha_clamps_to_seven_and_tolerates_short_input() {
    assert_eq!(short_git_sha("4f38b4f02b6a"), "4f38b4f");
    assert_eq!(short_git_sha("abc"), "abc");
    assert_eq!(short_git_sha(""), "");
}

#[test]
fn image_name_is_agent_independent() {
    // The derived image installs every supported agent, so its tag is keyed on
    // the role (and branch), never the selected agent — one image is reused
    // across all agents instead of forking a redundant per-agent copy.
    let flat = RoleSelector::new(None, "the-architect");
    assert_eq!(image_name(&flat, None), "jk_the-architect");
    assert_eq!(
        image_name_for_branch(&flat, "feat/scope", None),
        "jk_the-architect_feat-scope"
    );
}

#[test]
fn image_name_for_branch_lowercases_uppercase_branch() {
    let namespaced = RoleSelector::new(Some("chainargos"), "agent-brown");
    let flat = RoleSelector::new(None, "the-architect");
    assert_eq!(
        image_name_for_branch(&namespaced, "Feat/MY-PR", None),
        "jk_chainargos_agent-brown_feat-my-pr"
    );
    assert_eq!(
        image_name_for_branch(&flat, "RELEASE/1.0", None),
        "jk_the-architect_release-1.0"
    );
}

#[test]
fn format_agent_display_appends_instance_id() {
    assert_eq!(
        format_role_display("jk-k7p9m2xq-thearchitect", "The Architect"),
        "The Architect (k7p9m2xq)"
    );
}

#[test]
fn format_agent_display_falls_back_to_container_name() {
    assert_eq!(
        format_role_display("jk-k7p9m2xq-thearchitect", ""),
        "jk-k7p9m2xq-thearchitect"
    );
}
