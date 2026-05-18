use std::path::{Path, PathBuf};
use std::str::FromStr;

use dockerfile_parser_rs::{Dockerfile, Instruction};

use crate::repo::RoleRepoValidationError;

pub const CONSTRUCT_REGISTRY_IMAGE: &str = "projectjackin/construct";
pub const CONSTRUCT_STABLE_TAG: &str = "trixie";
/// Floating stable-channel tag. Default returned by `construct_image()` when
/// `JACKIN_CONSTRUCT_IMAGE` is unset; also used in `DockerfileNonConstruct`
/// error messages.
pub const CONSTRUCT_IMAGE: &str = "projectjackin/construct:trixie";

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
    /// (e.g. `0.1-trixie`). Compared against the `jackin.construct_version`
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
        final_stage_image: image_str.to_string(),
        final_stage_alias: alias.clone(),
        construct_version: tag.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn accepts_versioned_construct_with_alias() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            "FROM rust:1.95.0 AS builder\nRUN cargo build\n\n\
             FROM projectjackin/construct:0.1-trixie AS runtime\n\
             COPY --from=builder /app /workspace/app\n",
        )
        .unwrap();

        let validated = validate_agent_dockerfile(&dockerfile).unwrap();

        assert_eq!(
            validated.final_stage_image,
            "projectjackin/construct:0.1-trixie"
        );
        assert_eq!(validated.final_stage_alias.as_deref(), Some("runtime"));
        assert_eq!(validated.construct_version, "0.1-trixie");
    }

    #[test]
    fn accepts_versioned_construct_without_alias() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(&dockerfile, "FROM projectjackin/construct:0.2-trixie\n").unwrap();

        let validated = validate_agent_dockerfile(&dockerfile).unwrap();

        assert_eq!(validated.construct_version, "0.2-trixie");
    }

    #[test]
    fn accepts_digest_pinned_versioned_construct() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            "FROM projectjackin/construct:0.1-trixie@sha256:0b076bfbc53d36794fe54b1a9cab670f85f831af86d78426b1a88a8ac192d445\n",
        )
        .unwrap();

        let validated = validate_agent_dockerfile(&dockerfile).unwrap();

        // construct_version carries only the version tag, not the digest
        assert_eq!(validated.construct_version, "0.1-trixie");
    }

    #[test]
    fn rejects_floating_stable_tag() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(&dockerfile, format!("FROM {CONSTRUCT_IMAGE}\n")).unwrap();

        let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

        assert!(matches!(
            error,
            RoleRepoValidationError::DockerfileMissingVersionPin
        ));
        let msg = error.to_string();
        assert!(msg.contains("floating tag"));
        assert!(
            msg.contains("Renovate"),
            "error must include Renovate guidance; got: {msg}"
        );
    }

    #[test]
    fn rejects_empty_version_prefix() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            format!("FROM {CONSTRUCT_REGISTRY_IMAGE}:-{CONSTRUCT_STABLE_TAG}\n"),
        )
        .unwrap();

        let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

        assert!(matches!(
            error,
            RoleRepoValidationError::DockerfileMissingVersionPin
        ));
    }

    #[test]
    fn rejects_final_stage_on_other_image() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(&dockerfile, "FROM debian:trixie\n").unwrap();

        let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

        assert!(error.to_string().contains("projectjackin/construct:trixie"));
    }

    #[test]
    fn rejects_arg_indirection_in_final_from() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            r"ARG BASE=projectjackin/construct:trixie
FROM ${BASE}
",
        )
        .unwrap();

        let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("literal FROM projectjackin/construct:trixie")
        );
    }
}
