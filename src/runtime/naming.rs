//! Naming conventions, Docker label/filter constants, and lightweight identifier helpers.

use crate::instance::naming::CONTAINER_PREFIX_DASH;
use crate::instance::runtime_slug;
use crate::selector::RoleSelector;

// ── Docker label keys ─────────────────────────────────────────────────────
//
// Used to tag and filter jackin-managed containers and networks.

/// Applied to role containers, `DinD` sidecars, and networks.
pub(super) const LABEL_MANAGED: &str = "jackin.managed=true";
/// Role containers only — distinguishes them from `DinD` sidecars.
pub(super) const LABEL_KIND_ROLE: &str = "jackin.kind=role";
/// `DinD` sidecars only — distinguishes them from role containers.
pub(super) const LABEL_KIND_DIND: &str = "jackin.kind=dind";
/// Filter expression for `docker images --filter` to list jackin-managed role images.
pub(super) const FILTER_IMAGES: &str = "reference=jk-*";
/// Filter expression for `docker ps --filter` to find managed containers.
pub(super) const FILTER_MANAGED: &str = "label=jackin.managed=true";
/// Filter expression for `docker ps --filter` to find role containers.
pub(super) const FILTER_KIND_ROLE: &str = "label=jackin.kind=role";
/// Filter expression for `docker ps --filter` to find `DinD` sidecars.
pub(super) const FILTER_KIND_DIND: &str = "label=jackin.kind=dind";
/// Filter expression for `docker ps --filter` to find roles whose
/// workspace opted into the keep-awake reconciler.
pub(super) const FILTER_KEEP_AWAKE: &str = "label=jackin.keep_awake=true";
/// Applied to role containers whose workspace opted into the
/// keep-awake reconciler. Read by `runtime::caffeinate::reconcile`
/// to decide whether to keep `caffeinate` running.
pub(super) const LABEL_KEEP_AWAKE: &str = "jackin.keep_awake=true";

/// Format a human-friendly role name from a container name and its display label.
///
/// Examples:
///   - `("jk-k7p9m2xq-thearchitect", "The Architect")` → `"The Architect (k7p9m2xq)"`
///   - `("jk-k7p9m2xq-thearchitect", "")` → `"jk-k7p9m2xq-thearchitect"`
///
/// The instance-ID suffix is appended so two concurrent sessions of the
/// same role render as distinct rows in operator output.
pub(super) fn format_role_display(container_name: &str, display_name: &str) -> String {
    if display_name.is_empty() {
        return container_name.to_string();
    }
    crate::instance::naming::instance_id_from_container_base(container_name).map_or_else(
        || display_name.to_string(),
        |instance_id| format!("{display_name} ({instance_id})"),
    )
}

pub fn matching_family(selector: &RoleSelector, names: &[String]) -> Vec<String> {
    let role_slug = crate::instance::naming::compact_component(&selector.name, "role");
    names
        .iter()
        .filter(|name| crate::instance::naming::class_family_matches_with_slug(&role_slug, name))
        .cloned()
        .collect()
}

pub(super) fn image_name(selector: &RoleSelector) -> String {
    format!("{CONTAINER_PREFIX_DASH}{}", runtime_slug(selector))
}

/// Image tag for a branch-specific local build. Branch slashes become dashes
/// so the tag is a valid Docker name and does not overwrite the stable image
/// (e.g. `jk-the-architect-feat-my-pr`).
pub(super) fn image_name_for_branch(selector: &RoleSelector, branch: &str) -> String {
    let slug = branch.replace('/', "-").to_ascii_lowercase();
    format!("{CONTAINER_PREFIX_DASH}{}-{slug}", runtime_slug(selector))
}

/// Docker volume name for the TLS client certificates shared between the
/// `DinD` sidecar (writer) and the role container (reader).
pub(super) fn dind_certs_volume(container_name: &str) -> String {
    format!("{container_name}-dind-certs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_name_distinguishes_namespaced_and_flat_classes() {
        let namespaced = crate::selector::RoleSelector::new(Some("chainargos"), "agent-brown");
        let flat = crate::selector::RoleSelector::new(None, "chainargos-agent-brown");
        assert_ne!(image_name(&namespaced), image_name(&flat));
        assert_eq!(image_name(&namespaced), "jk-chainargos_agent-brown");
        assert_eq!(image_name(&flat), "jk-chainargos-agent-brown");
    }

    #[test]
    fn image_name_for_branch_substitutes_slashes_and_keeps_prefix() {
        let namespaced = crate::selector::RoleSelector::new(Some("chainargos"), "agent-brown");
        let flat = crate::selector::RoleSelector::new(None, "the-architect");

        assert_eq!(
            image_name_for_branch(&namespaced, "feat/my-pr"),
            "jk-chainargos_agent-brown-feat-my-pr"
        );
        assert_eq!(
            image_name_for_branch(&flat, "main"),
            "jk-the-architect-main"
        );
        // Branch with multiple slashes — all become dashes.
        assert_eq!(
            image_name_for_branch(&flat, "feat/scope/detail"),
            "jk-the-architect-feat-scope-detail"
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
}
