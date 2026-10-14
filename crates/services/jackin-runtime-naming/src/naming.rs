// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::RoleSelector;

pub use jackin_image::naming::{
    HOST_IDENTITY_STRATEGY, IMAGE_PREFIX, LABEL_IMAGE_AGENT_VERSION_PREFIX,
    LABEL_IMAGE_CAPSULE_VERSION, LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION,
    LABEL_IMAGE_MANIFEST_VERSION, LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION,
    LABEL_IMAGE_ROLE_GIT_SHA, image_name, image_name_for_branch, role_base_image_name,
    short_git_sha,
};

pub const LABEL_MANAGED: &str = "jackin.managed=true";

pub const LABEL_KIND_ROLE: &str = "jackin.kind=role";

pub const LABEL_KIND_DIND: &str = "jackin.kind=dind";

pub const LABEL_KIND_PREWARM_DIND: &str = "jackin.kind=prewarm-dind";

pub const LABEL_PREWARM: &str = "jackin.prewarm=true";

pub const LABEL_KEEP_AWAKE: &str = "jackin.keep.awake=true";

pub const LABEL_ROLE_KEY: &str = "jackin.role";

pub const LABEL_IMAGE_KEY: &str = "jackin.image";

pub fn format_role_display(container_name: &str, display_name: &str) -> String {
    if display_name.is_empty() {
        return container_name.to_owned();
    }
    jackin_instance::naming::instance_id_from_container_base(container_name).map_or_else(
        || display_name.to_owned(),
        |instance_id| format!("{display_name} ({instance_id})"),
    )
}

pub fn matching_family(selector: &RoleSelector, names: &[String]) -> Vec<String> {
    let role_slug = jackin_instance::naming::compact_component(&selector.name, "role");
    names
        .iter()
        .filter(|name| jackin_instance::naming::class_family_matches_with_slug(&role_slug, name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
