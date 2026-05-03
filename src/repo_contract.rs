use std::path::{Path, PathBuf};
use std::str::FromStr;

use dockerfile_parser_rs::{Dockerfile, Instruction};

pub const CONSTRUCT_IMAGE: &str = "projectjackin/construct:trixie";

#[derive(Debug, Clone)]
pub struct ValidatedDockerfile {
    pub dockerfile_path: PathBuf,
    pub dockerfile_contents: String,
    pub final_stage_image: String,
    pub final_stage_alias: Option<String>,
}

pub fn validate_agent_dockerfile(dockerfile_path: &Path) -> anyhow::Result<ValidatedDockerfile> {
    let dockerfile_contents = std::fs::read_to_string(dockerfile_path)?;
    let dockerfile = Dockerfile::from_str(&dockerfile_contents).map_err(|error| {
        anyhow::anyhow!("invalid agent repo: unable to parse Dockerfile: {error}")
    })?;

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
        anyhow::bail!("invalid agent repo: Dockerfile must contain at least one FROM instruction")
    };

    anyhow::ensure!(
        platform.is_none() && image == CONSTRUCT_IMAGE,
        "invalid agent repo: final Dockerfile stage must use literal FROM {CONSTRUCT_IMAGE}"
    );

    Ok(ValidatedDockerfile {
        dockerfile_path: dockerfile_path.to_path_buf(),
        dockerfile_contents,
        final_stage_image: image.clone(),
        final_stage_alias: alias.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn accepts_final_stage_on_construct_image() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            r"FROM rust:1.95.0 AS builder
RUN cargo build

FROM projectjackin/construct:trixie AS runtime
COPY --from=builder /app /workspace/app
",
        )
        .unwrap();

        let validated = validate_agent_dockerfile(&dockerfile).unwrap();

        assert_eq!(validated.final_stage_image, CONSTRUCT_IMAGE);
        assert_eq!(validated.final_stage_alias.as_deref(), Some("runtime"));
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
