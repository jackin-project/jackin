// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::str::FromStr;

use dockerfile_parser_rs::{Dockerfile, Instruction};

use crate::repo::RoleRepoValidationError;

pub use jackin_core::DOCKERFILE_NAME;

pub use jackin_core::MANIFEST_FILENAME;

pub const CONSTRUCT_REGISTRY_IMAGE: &str = "projectjackin/construct";

pub const CONSTRUCT_STABLE_TAG: &str = "trixie";

pub const CONSTRUCT_IMAGE: &str = "projectjackin/construct:trixie";

pub const CONSTRUCT_PINNED_TAG: &str = "0.1-trixie";

pub const BASE_DOCKERFILE_FROM: &str = "FROM projectjackin/construct:0.1-trixie\n";

pub const LABEL_PUBLISHED_IMAGE_CONSTRUCT_VERSION: &str = "jackin.construct.version";

pub const LABEL_PUBLISHED_IMAGE_ROLE_GIT_SHA: &str = "jackin.role.git.sha";

pub fn published_image_labels(construct_version: &str, role_git_sha: &str) -> [String; 2] {
    [
        format!("{LABEL_PUBLISHED_IMAGE_CONSTRUCT_VERSION}={construct_version}"),
        format!("{LABEL_PUBLISHED_IMAGE_ROLE_GIT_SHA}={role_git_sha}"),
    ]
}

pub fn published_image_repository(published_image: &str) -> &str {
    let without_digest = published_image
        .split_once('@')
        .map_or(published_image, |(base, _)| base);
    let last_slash = without_digest.rfind('/').unwrap_or(0);
    match without_digest.rfind(':') {
        Some(colon) if colon > last_slash => without_digest.get(..colon).unwrap_or(without_digest),
        _ => without_digest,
    }
}

pub fn construct_image() -> String {
    std::env::var("JACKIN_CONSTRUCT_IMAGE").unwrap_or_else(|_| CONSTRUCT_IMAGE.to_owned())
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ValidatedDockerfile {
    pub dockerfile_path: PathBuf,

    pub dockerfile_contents: String,

    pub final_stage_image: String,

    pub final_stage_alias: Option<String>,

    pub construct_version: String,
}

pub fn validate_agent_dockerfile(
    dockerfile_path: &Path,
) -> Result<ValidatedDockerfile, RoleRepoValidationError> {
    let dockerfile_contents = std::fs::read_to_string(dockerfile_path)?;
    let dockerfile = Dockerfile::from_str(&dockerfile_contents)
        .map_err(|error| RoleRepoValidationError::DockerfileParse(error.to_string()))?;

    let Some((platform, image, alias)) =
        dockerfile
            .instructions
            .iter()
            .rev()
            .find_map(|instruction| {
                let Instruction::From {
                    platform,
                    image,
                    alias,
                } = instruction
                else {
                    return None;
                };

                Some((platform, image, alias))
            })
    else {
        return Err(RoleRepoValidationError::DockerfileMissingFrom);
    };

    let image_str = image.as_str();

    let base_ref = image_str
        .split_once('@')
        .map_or(image_str, |(base, _)| base);
    let (registry_image, tag) = base_ref.rsplit_once(':').unwrap_or((base_ref, ""));

    let expected = CONSTRUCT_IMAGE.to_owned();
    if platform.is_some() || registry_image != CONSTRUCT_REGISTRY_IMAGE {
        return Err(RoleRepoValidationError::DockerfileNonConstruct { expected });
    }

    let version_suffix = format!("-{CONSTRUCT_STABLE_TAG}");
    if tag
        .strip_suffix(version_suffix.as_str())
        .is_none_or(str::is_empty)
    {
        return Err(RoleRepoValidationError::DockerfileMissingVersionPin);
    }

    Ok(ValidatedDockerfile {
        dockerfile_path: dockerfile_path.to_path_buf(),
        dockerfile_contents,
        final_stage_image: image_str.to_owned(),
        final_stage_alias: alias.clone(),
        construct_version: tag.to_owned(),
    })
}

#[cfg(test)]
mod tests;
