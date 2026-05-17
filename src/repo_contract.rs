use std::path::{Path, PathBuf};
use std::str::FromStr;

use dockerfile_parser_rs::{Dockerfile, Instruction};

use crate::repo::RoleRepoValidationError;

pub const CONSTRUCT_REGISTRY_IMAGE: &str = "projectjackin/construct";
pub const CONSTRUCT_STABLE_TAG: &str = "trixie";
/// Floating tag — kept for the JACKIN_CONSTRUCT_IMAGE default and error messages.
pub const CONSTRUCT_IMAGE: &str = "projectjackin/construct:trixie";

pub fn construct_image() -> String {
    std::env::var("JACKIN_CONSTRUCT_IMAGE").unwrap_or_else(|_| CONSTRUCT_IMAGE.to_owned())
}

#[derive(Debug, Clone)]
pub struct ValidatedDockerfile {
    pub dockerfile_path: PathBuf,
    pub dockerfile_contents: String,
    /// Full versioned image tag (e.g. `projectjackin/construct:0.1-trixie`).
    pub final_stage_image: String,
    pub final_stage_alias: Option<String>,
    /// The versioned tag component (e.g. `0.1-trixie`). Stored in the
    /// published image label so jackin can detect staleness at launch time.
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
    // "projectjackin/construct:0.1-trixie" → ("projectjackin/construct", "0.1-trixie")
    let (registry_image, tag) = base_ref.rsplit_once(':').unwrap_or((base_ref, ""));

    let expected = CONSTRUCT_IMAGE.to_owned();
    if platform.is_some() || registry_image != CONSTRUCT_REGISTRY_IMAGE {
        return Err(RoleRepoValidationError::DockerfileNonConstruct { expected });
    }

    // The floating stable tag is not allowed — role Dockerfiles must pin to a
    // versioned release (e.g. "0.1-trixie") so Renovate can track updates and
    // jackin can detect published-image staleness at launch time.
    let version_suffix = format!("-{CONSTRUCT_STABLE_TAG}");
    if tag == CONSTRUCT_STABLE_TAG || !tag.ends_with(&version_suffix) {
        return Err(RoleRepoValidationError::DockerfileMissingVersionPin {
            image: CONSTRUCT_REGISTRY_IMAGE.to_owned(),
            stable_tag: CONSTRUCT_STABLE_TAG.to_owned(),
        });
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
            RoleRepoValidationError::DockerfileMissingVersionPin { .. }
        ));
        assert!(error.to_string().contains("floating tag"));
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
