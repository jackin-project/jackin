//! Naming conventions, Docker label/filter constants, and lightweight identifier helpers.

use crate::instance::runtime_slug;
use crate::selector::RoleSelector;

/// Prefix for jackin-managed Docker image names.
///
/// Uses `_` as the separator so all structural boundaries in an image name
/// are `_`, visually distinct from container names which use `jk-{id}-…`.
pub(super) const IMAGE_PREFIX: &str = "jk_";

// ── Docker label keys ─────────────────────────────────────────────────────
//
// Used to tag and filter jackin-managed containers and networks.

/// Applied to role containers, `DinD` sidecars, and networks.
pub(super) const LABEL_MANAGED: &str = "jackin.managed=true";
/// Role containers only — distinguishes them from `DinD` sidecars.
pub(super) const LABEL_KIND_ROLE: &str = "jackin.kind=role";
/// `DinD` sidecars only — distinguishes them from role containers.
pub(super) const LABEL_KIND_DIND: &str = "jackin.kind=dind";
/// Applied to role containers whose workspace opted into the
/// keep-awake reconciler. Read by `runtime::caffeinate::reconcile`
/// to decide whether to keep `caffeinate` running.
pub(super) const LABEL_KEEP_AWAKE: &str = "jackin.keep_awake=true";

/// Image label key recording which construct image was used to build a
/// derived image. Checked at load time: a mismatch between this label and
/// `JACKIN_CONSTRUCT_IMAGE` (or the canonical default) forces a rebuild so a
/// derived image built against a local construct is never reused by a standard
/// jackin invocation (and vice versa).
pub(super) const LABEL_IMAGE_CONSTRUCT: &str = "jackin.construct_image";

/// Image label key recording the construct version tag baked into a published
/// role image. Role CI calls `jackin-role construct-version` to obtain the tag,
/// passes it as `--build-arg CONSTRUCT_VERSION=<ver>` to `docker build`, and
/// the Dockerfile's `LABEL jackin.construct_version=${CONSTRUCT_VERSION}`
/// instruction writes the image label (see jackin-role-action). Checked at
/// launch time: a mismatch against the Dockerfile's pinned version means the
/// published image pre-dates a Renovate bump; jackin falls back to workspace
/// mode so the role's workspace Dockerfile — carrying the new pin — is used.
pub(super) const LABEL_IMAGE_CONSTRUCT_VERSION: &str = "jackin.construct_version";

/// Image label key recording the git commit SHA of the role repo from which a
/// published image was built. Role CI passes `--build-arg ROLE_GIT_SHA=<sha>`
/// (set to `${{ github.sha }}`) so the Dockerfile's
/// `LABEL jackin.role_git_sha=${ROLE_GIT_SHA}` bakes it in. Checked at launch
/// time before the construct-version check: if the label matches the HEAD of
/// the cached role repo the image is current and the workspace rebuild is
/// skipped. Falls through to the construct-version check when this label is
/// absent (images predating this feature).
pub(super) const LABEL_IMAGE_ROLE_GIT_SHA: &str = "jackin.role_git_sha";

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
    format!("{IMAGE_PREFIX}{}", runtime_slug(selector))
}

/// Image tag for a branch-specific local build. Branch slashes become dashes
/// so the tag is a valid Docker name and does not overwrite the stable image
/// (e.g. `jk_the-architect_feat-my-pr`). All structural separators in image
/// names are `_`. Role names and branch slugs contain only `[a-z0-9-]`, so
/// `_` marks every boundary.
pub(super) fn image_name_for_branch(selector: &RoleSelector, branch: &str) -> String {
    let slug = branch.replace('/', "-").to_ascii_lowercase();
    format!("{IMAGE_PREFIX}{}_{slug}", runtime_slug(selector))
}

/// Docker volume name for the TLS client certificates shared between the
/// `DinD` sidecar (writer) and the role container (reader).
pub(super) fn dind_certs_volume(container_name: &str) -> String {
    format!("{container_name}-dind-certs")
}

pub(super) fn dind_container_name(container_name: &str) -> String {
    format!("{container_name}-dind")
}

pub(super) fn role_network_name(container_name: &str) -> String {
    format!("{container_name}-net")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_name_distinguishes_namespaced_and_flat_classes() {
        let namespaced = crate::selector::RoleSelector::new(Some("chainargos"), "agent-brown");
        let flat = crate::selector::RoleSelector::new(None, "chainargos-agent-brown");
        assert_ne!(image_name(&namespaced), image_name(&flat));
        assert_eq!(image_name(&namespaced), "jk_chainargos_agent-brown");
        assert_eq!(image_name(&flat), "jk_chainargos-agent-brown");
    }

    #[test]
    fn image_name_flat_role_uses_jk_underscore_prefix() {
        let flat = crate::selector::RoleSelector::new(None, "agent-smith");
        assert_eq!(image_name(&flat), "jk_agent-smith");
    }

    #[test]
    fn image_name_for_branch_substitutes_slashes_and_keeps_prefix() {
        let namespaced = crate::selector::RoleSelector::new(Some("chainargos"), "agent-brown");
        let flat = crate::selector::RoleSelector::new(None, "the-architect");

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
    fn image_name_for_branch_lowercases_uppercase_branch() {
        let namespaced = crate::selector::RoleSelector::new(Some("chainargos"), "agent-brown");
        let flat = crate::selector::RoleSelector::new(None, "the-architect");
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
}
