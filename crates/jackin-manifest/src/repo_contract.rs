//! Dockerfile validation rules: ensure the role Dockerfile extends `projectjackin/construct`.
//!
//! Produces `ValidatedDockerfile` whose fields are guaranteed by the
//! invariants enforced here; external crates cannot construct it via struct
//! literal. Not responsible for image build or tag logic — those live in
//! `runtime::image`.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use dockerfile_parser_rs::{Dockerfile, Instruction};

use crate::repo::RoleRepoValidationError;

pub use jackin_core::constants::DOCKERFILE_NAME;
/// Re-exported from `jackin-core` — callers use `crate::repo_contract::MANIFEST_FILENAME`
/// for backward compat while the canonical definition lives in the leaf crate.
pub use jackin_core::constants::MANIFEST_FILENAME;

pub const CONSTRUCT_REGISTRY_IMAGE: &str = "projectjackin/construct";
pub const CONSTRUCT_STABLE_TAG: &str = "trixie";
/// Floating stable-channel tag. Default returned by `construct_image()` when
/// `JACKIN_CONSTRUCT_IMAGE` is unset; also used in `DockerfileNonConstruct`
/// error messages.
pub const CONSTRUCT_IMAGE: &str = "projectjackin/construct:trixie";
/// Pinned construct tag used in generated Dockerfiles and test fixtures.
///
/// Role Dockerfiles must pin to a versioned release like this so Renovate
/// can track updates and jackin can detect published-image staleness.
pub const CONSTRUCT_PINNED_TAG: &str = "0.1-trixie";
/// Canonical `FROM` line used in generated Dockerfiles and test harness fixtures.
pub const BASE_DOCKERFILE_FROM: &str = "FROM projectjackin/construct:0.1-trixie\n";

/// Published role-image label storing the construct tag from the role Dockerfile.
pub const LABEL_PUBLISHED_IMAGE_CONSTRUCT_VERSION: &str = "jackin.construct.version";
/// Published role-image label storing the role repository commit SHA.
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
        Some(colon) if colon > last_slash => &without_digest[..colon],
        _ => without_digest,
    }
}

pub fn construct_image() -> String {
    std::env::var("JACKIN_CONSTRUCT_IMAGE").unwrap_or_else(|_| CONSTRUCT_IMAGE.to_owned())
}

/// All instances carry the invariants enforced by `validate_agent_dockerfile`;
/// external crates cannot construct this type with struct-literal syntax.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ValidatedDockerfile {
    pub dockerfile_path: PathBuf,
    pub dockerfile_contents: String,
    /// Full versioned image reference from the final `FROM` line
    /// (e.g. `projectjackin/construct:0.1-trixie`). Digest pins are included.
    pub final_stage_image: String,
    pub final_stage_alias: Option<String>,
    /// Tag component of `final_stage_image` with any digest pin stripped
    /// (e.g. `0.1-trixie`). Compared against the `jackin.construct.version`
    /// label on the published image to detect staleness at launch time.
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
    // Strip optional digest pin: "image:tag@sha256:..." → "image:tag"
    // Renovate's docker:pinDigests preset appends @sha256:... after the tag;
    // validation must look through it to reach the version tag.
    let base_ref = image_str
        .split_once('@')
        .map_or(image_str, |(base, _)| base);
    let (registry_image, tag) = base_ref.rsplit_once(':').unwrap_or((base_ref, ""));

    let expected = CONSTRUCT_IMAGE.to_owned();
    if platform.is_some() || registry_image != CONSTRUCT_REGISTRY_IMAGE {
        return Err(RoleRepoValidationError::DockerfileNonConstruct { expected });
    }

    // The floating stable tag is not allowed — role Dockerfiles must pin to a
    // versioned release (e.g. "0.1-trixie") so Renovate can track updates and
    // jackin can detect published-image staleness at launch time.
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
