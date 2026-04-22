//! Naming conventions, Docker label/filter constants, and lightweight identifier helpers.

use crate::selector::ClassSelector;

// ── Docker label keys ─────────────────────────────────────────────────────
//
// Used to tag and filter jackin-managed containers and networks.

/// Applied to agent containers, `DinD` sidecars, and networks.
pub(super) const LABEL_MANAGED: &str = "jackin.managed=true";
/// Agent containers only — distinguishes them from `DinD` sidecars.
pub(super) const LABEL_ROLE_AGENT: &str = "jackin.role=agent";
/// `DinD` sidecars only — distinguishes them from agent containers.
pub(super) const LABEL_ROLE_DIND: &str = "jackin.role=dind";
/// Filter expression for `docker ps --filter` to find managed containers.
pub(super) const FILTER_MANAGED: &str = "label=jackin.managed=true";
/// Filter expression for `docker ps --filter` to find agent containers.
pub(super) const FILTER_ROLE_AGENT: &str = "label=jackin.role=agent";
/// Filter expression for `docker ps --filter` to find `DinD` sidecars.
pub(super) const FILTER_ROLE_DIND: &str = "label=jackin.role=dind";

/// Format a human-friendly agent name from a container name and its display label.
///
/// Examples:
///   - `("jackin-the-architect", "The Architect")` → `"The Architect"`
///   - `("jackin-the-architect-clone-2", "The Architect")` → `"The Architect (Clone 2)"`
///   - `("jackin-the-architect", "")` → `"jackin-the-architect"`
pub(super) fn format_agent_display(container_name: &str, display_name: &str) -> String {
    if display_name.is_empty() {
        return container_name.to_string();
    }

    container_name.rsplit_once("-clone-").map_or_else(
        || display_name.to_string(),
        |suffix| format!("{display_name} (Clone {})", suffix.1),
    )
}

pub fn matching_family(selector: &ClassSelector, names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| crate::instance::class_family_matches(selector, name))
        .cloned()
        .collect()
}

pub(super) fn image_name(selector: &ClassSelector) -> String {
    format!("jackin-{}", crate::instance::runtime_slug(selector))
}

/// Docker volume name for the TLS client certificates shared between the
/// `DinD` sidecar (writer) and the agent container (reader).
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
        let namespaced = ClassSelector::new(Some("chainargos"), "the-architect");
        let flat = ClassSelector::new(None, "chainargos-the-architect");

        assert_ne!(image_name(&namespaced), image_name(&flat));
    }

    #[test]
    fn format_agent_display_uses_display_name_for_primary() {
        assert_eq!(
            format_agent_display("jackin-the-architect", "The Architect"),
            "The Architect"
        );
    }

    #[test]
    fn format_agent_display_appends_clone_index() {
        assert_eq!(
            format_agent_display("jackin-the-architect-clone-2", "The Architect"),
            "The Architect (Clone 2)"
        );
    }

    #[test]
    fn format_agent_display_falls_back_to_container_name() {
        assert_eq!(
            format_agent_display("jackin-the-architect", ""),
            "jackin-the-architect"
        );
    }
}
