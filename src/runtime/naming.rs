//! Naming conventions, Docker label/filter constants, and lightweight identifier helpers.

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
///   - `("jackin-the-architect", "The Architect")` → `"The Architect"`
///   - `("jackin-the-architect-clone-2", "The Architect")` → `"The Architect (Clone 2)"`
///   - `("jackin-the-architect", "")` → `"jackin-the-architect"`
pub(super) fn format_role_display(container_name: &str, display_name: &str) -> String {
    if display_name.is_empty() {
        return container_name.to_string();
    }

    container_name.rsplit_once("-clone-").map_or_else(
        || display_name.to_string(),
        |suffix| format!("{display_name} (Clone {})", suffix.1),
    )
}

pub fn matching_family(selector: &RoleSelector, names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| crate::instance::class_family_matches(selector, name))
        .cloned()
        .collect()
}

pub(super) fn image_name(selector: &RoleSelector) -> String {
    format!("jackin-{}", crate::instance::runtime_slug(selector))
}

/// Image tag for a branch-specific local build. Branch slashes become dashes
/// so the tag is a valid Docker name and does not overwrite the stable image
/// (e.g. `jackin-the-architect-feat-my-pr`).
pub(super) fn image_name_for_branch(selector: &RoleSelector, branch: &str) -> String {
    let slug = branch.replace('/', "-").to_ascii_lowercase();
    format!("jackin-{}-{slug}", crate::instance::runtime_slug(selector))
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
    fn dind_certs_volume_derives_from_container_name() {
        assert_eq!(
            dind_certs_volume("jackin-agent-smith"),
            "jackin-agent-smith-dind-certs"
        );
        assert_eq!(
            dind_certs_volume("jackin-chainargos__the-architect-clone-2"),
            "jackin-chainargos__the-architect-clone-2-dind-certs"
        );
    }

    #[test]
    fn image_name_distinguishes_namespaced_and_flat_classes() {
        let namespaced = RoleSelector::new(Some("chainargos"), "the-architect");
        let flat = RoleSelector::new(None, "chainargos-the-architect");

        assert_ne!(image_name(&namespaced), image_name(&flat));
    }

    #[test]
    fn format_agent_display_uses_display_name_for_primary() {
        assert_eq!(
            format_role_display("jackin-the-architect", "The Architect"),
            "The Architect"
        );
    }

    #[test]
    fn format_agent_display_appends_clone_index() {
        assert_eq!(
            format_role_display("jackin-the-architect-clone-2", "The Architect"),
            "The Architect (Clone 2)"
        );
    }

    #[test]
    fn format_agent_display_falls_back_to_container_name() {
        assert_eq!(
            format_role_display("jackin-the-architect", ""),
            "jackin-the-architect"
        );
    }
}
