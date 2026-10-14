// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub const MANIFEST_FILENAME: &str = "jackin.role.toml";

pub const DOCKERFILE_NAME: &str = "Dockerfile";

pub const CLAUDE_OAUTH_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

pub const CURRENT_MANIFEST_VERSION: &str = "v1alpha7";

pub fn current_manifest_version() -> String {
    CURRENT_MANIFEST_VERSION.to_owned()
}

pub const CONTAINER_PREFIX: &str = "jk";

pub const CONTAINER_PREFIX_DASH: &str = "jk-";

#[must_use]
pub fn instance_id_from_container_base(container_base: &str) -> Option<&str> {
    container_base
        .strip_prefix(CONTAINER_PREFIX_DASH)?
        .split_once('-')
        .map(|(id, _)| id)
}
