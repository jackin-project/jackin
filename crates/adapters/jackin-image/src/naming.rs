// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::RoleSelector;
use jackin_core::runtime_slug;

pub const IMAGE_PREFIX: &str = "jk_";

pub const LABEL_IMAGE_CONSTRUCT: &str = "jackin.construct.image";

pub const LABEL_IMAGE_CONSTRUCT_VERSION: &str =
    jackin_manifest::LABEL_PUBLISHED_IMAGE_CONSTRUCT_VERSION;

pub const LABEL_IMAGE_ROLE_GIT_SHA: &str = jackin_manifest::LABEL_PUBLISHED_IMAGE_ROLE_GIT_SHA;

pub const LABEL_IMAGE_RECIPE_HASH: &str = "jackin.image.recipe.hash";

pub const LABEL_IMAGE_RECIPE_VERSION: &str = "jackin.image.recipe.version";

pub const LABEL_IMAGE_AGENT_VERSION_PREFIX: &str = "jackin.agent";

pub const LABEL_IMAGE_CAPSULE_VERSION: &str = "jackin.capsule.version";

pub const LABEL_IMAGE_MANIFEST_VERSION: &str = "jackin.manifest.version";

pub const HOST_IDENTITY_STRATEGY: &str = "host-uid-owned-runtime-home-v1";

const SHORT_GIT_SHA_LEN: usize = 7;

pub fn short_git_sha(sha: &str) -> &str {
    &sha[..sha.len().min(SHORT_GIT_SHA_LEN)]
}

fn tag_with_sha(name: String, role_git_sha: Option<&str>) -> String {
    match role_git_sha.filter(|sha| !sha.is_empty() && sha.bytes().all(|b| b.is_ascii_hexdigit())) {
        Some(sha) => format!("{name}:{}", short_git_sha(sha)),
        None => name,
    }
}

pub fn image_name(selector: &RoleSelector, role_git_sha: Option<&str>) -> String {
    tag_with_sha(
        format!("{IMAGE_PREFIX}{}", runtime_slug(selector)),
        role_git_sha,
    )
}

pub fn image_name_for_branch(
    selector: &RoleSelector,
    branch: &str,
    role_git_sha: Option<&str>,
) -> String {
    let slug = branch.replace('/', "-").to_ascii_lowercase();
    tag_with_sha(
        format!("{IMAGE_PREFIX}{}_{slug}", runtime_slug(selector)),
        role_git_sha,
    )
}

pub fn role_base_image_name(
    selector: &RoleSelector,
    branch: Option<&str>,
    role_git_sha: Option<&str>,
) -> String {
    let repo = match branch {
        Some(b) => {
            let slug = b.replace('/', "-").to_ascii_lowercase();
            format!("{IMAGE_PREFIX}{}_{slug}__base", runtime_slug(selector))
        }
        None => format!("{IMAGE_PREFIX}{}__base", runtime_slug(selector)),
    };
    tag_with_sha(repo, role_git_sha)
}
