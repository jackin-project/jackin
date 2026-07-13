// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Naming conventions, Docker label/filter constants, and lightweight identifier helpers.
//!
//! Image-tag and label constants for jackin-built images (`IMAGE_PREFIX`,
//! `LABEL_IMAGE_*`, `image_name` / `image_name_for_branch` / `role_base_image_name`
//! / `short_git_sha` / `tag_with_sha`) moved to `jackin-image::naming` per D1.
//! Re-exported at the `runtime::image` module's edge so existing
//! `crate::runtime::image::X` call sites compile unchanged.

use jackin_core::selector::RoleSelector;

pub use jackin_image::naming::{
    HOST_IDENTITY_STRATEGY, IMAGE_PREFIX, LABEL_IMAGE_AGENT_VERSION_PREFIX,
    LABEL_IMAGE_CAPSULE_VERSION, LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION,
    LABEL_IMAGE_MANIFEST_VERSION, LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION,
    LABEL_IMAGE_ROLE_GIT_SHA, image_name, image_name_for_branch, role_base_image_name,
    short_git_sha,
};

// ── Docker label keys (not image-specific — stay in runtime/naming) ────

/// Applied to role containers, `DinD` sidecars, and networks.
pub const LABEL_MANAGED: &str = "jackin.managed=true";
/// Role containers only — distinguishes them from `DinD` sidecars.
pub(super) const LABEL_KIND_ROLE: &str = "jackin.kind=role";
/// `DinD` sidecars only — distinguishes them from role containers.
pub(super) const LABEL_KIND_DIND: &str = "jackin.kind=dind";
/// Explicit prewarmed `DinD` sidecars. These are not attached to a role
/// container yet, so orphan GC must not treat them as role-owned sidecars.
pub(super) const LABEL_KIND_PREWARM_DIND: &str = "jackin.kind=prewarm-dind";
/// Diagnostic label for reusable prewarm resources owned by jackin.
pub(super) const LABEL_PREWARM: &str = "jackin.prewarm=true";
/// Applied to role containers whose workspace opted into the
/// keep-awake reconciler. Read by `runtime::caffeinate::reconcile`
/// to decide whether to keep `caffeinate` running.
pub(super) const LABEL_KEEP_AWAKE: &str = "jackin.keep.awake=true";

/// Container label key storing the role container name. Applied to `DinD`
/// sidecars and managed networks so GC can map them back to their role.
pub(super) const LABEL_ROLE_KEY: &str = "jackin.role";

/// Container / image label key storing the derived image name. Applied to
/// role containers so image GC can skip images currently in use.
pub(super) const LABEL_IMAGE_KEY: &str = "jackin.image";

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
        return container_name.to_owned();
    }
    crate::instance::naming::instance_id_from_container_base(container_name).map_or_else(
        || display_name.to_owned(),
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

#[cfg(test)]
mod tests;
