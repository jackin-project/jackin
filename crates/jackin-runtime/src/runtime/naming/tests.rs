//! Tests for `naming`.
use super::*;

#[test]
fn image_name_distinguishes_namespaced_and_flat_classes() {
    let namespaced = RoleSelector::new(Some("chainargos"), "agent-brown");
    let flat = RoleSelector::new(None, "chainargos-agent-brown");
    assert_ne!(image_name(&namespaced), image_name(&flat));
    assert_eq!(image_name(&namespaced), "jk_chainargos_agent-brown");
    assert_eq!(image_name(&flat), "jk_chainargos-agent-brown");
}

#[test]
fn image_name_flat_role_uses_jk_underscore_prefix() {
    let flat = RoleSelector::new(None, "agent-smith");
    assert_eq!(image_name(&flat), "jk_agent-smith");
}

#[test]
fn image_name_for_branch_substitutes_slashes_and_keeps_prefix() {
    let namespaced = RoleSelector::new(Some("chainargos"), "agent-brown");
    let flat = RoleSelector::new(None, "the-architect");

    assert_eq!(
        image_name_for_branch(&namespaced, "feat/my-pr"),
        "jk_chainargos_agent-brown_feat-my-pr"
    );
    assert_eq!(
        image_name_for_branch(&flat, "main"),
        "jk_the-architect_main"
    );
    // Branch with multiple slashes — all become dashes.
    assert_eq!(
        image_name_for_branch(&flat, "feat/scope/detail"),
        "jk_the-architect_feat-scope-detail"
    );
}

#[test]
fn image_name_for_agent_scopes_selected_runtime_recipe() {
    let flat = RoleSelector::new(None, "the-architect");
    assert_eq!(
        image_name_for_agent(&flat, jackin_core::agent::Agent::Claude),
        "jk_the-architect_claude"
    );
    assert_eq!(
        image_name_for_agent(&flat, jackin_core::agent::Agent::Codex),
        "jk_the-architect_codex"
    );
    assert_eq!(
        image_name_for_branch_agent(&flat, "feat/scope", jackin_core::agent::Agent::Kimi),
        "jk_the-architect_feat-scope_kimi"
    );
}

#[test]
fn image_name_for_branch_lowercases_uppercase_branch() {
    let namespaced = RoleSelector::new(Some("chainargos"), "agent-brown");
    let flat = RoleSelector::new(None, "the-architect");
    assert_eq!(
        image_name_for_branch(&namespaced, "Feat/MY-PR"),
        "jk_chainargos_agent-brown_feat-my-pr"
    );
    assert_eq!(
        image_name_for_branch(&flat, "RELEASE/1.0"),
        "jk_the-architect_release-1.0"
    );
}

#[test]
fn dind_certs_volume_derives_from_container_name() {
    assert_eq!(
        dind_certs_volume("jk-agent-smith"),
        "jk-agent-smith-dind-certs"
    );
    assert_eq!(
        dind_certs_volume("jk-k7p9m2xq-chainargos-thearchitect"),
        "jk-k7p9m2xq-chainargos-thearchitect-dind-certs"
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
