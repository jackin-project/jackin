use dockerfile_parser::{Dockerfile, Instruction};
use std::path::{Path, PathBuf};

pub const CONSTRUCT_IMAGE: &str = "donbeave/jackin-construct:trixie";

#[derive(Debug, Clone)]
pub struct ValidatedDockerfile {
    pub dockerfile_path: PathBuf,
    pub dockerfile_contents: String,
    pub final_stage_image: String,
    pub final_stage_alias: Option<String>,
}

pub fn validate_agent_dockerfile(dockerfile_path: &Path) -> anyhow::Result<ValidatedDockerfile> {
    let dockerfile_contents = std::fs::read_to_string(dockerfile_path)?;
    let dockerfile = Dockerfile::parse(&dockerfile_contents).map_err(|error| {
        anyhow::anyhow!("invalid agent repo: unable to parse Dockerfile: {error}")
    })?;

    let final_stage = dockerfile.iter_stages().last().ok_or_else(|| {
        anyhow::anyhow!("invalid agent repo: Dockerfile must contain at least one FROM instruction")
    })?;

    let from = match final_stage.instructions.first() {
        Some(Instruction::From(from)) => from,
        _ => anyhow::bail!(
            "invalid agent repo: Dockerfile must contain at least one FROM instruction"
        ),
    };

    anyhow::ensure!(
        from.flags.is_empty() && from.image.as_ref() == CONSTRUCT_IMAGE,
        "invalid agent repo: final Dockerfile stage must use literal FROM {}",
        CONSTRUCT_IMAGE
    );

    Ok(ValidatedDockerfile {
        dockerfile_path: dockerfile_path.to_path_buf(),
        dockerfile_contents,
        final_stage_image: from.image.to_string(),
        final_stage_alias: from.alias.as_ref().map(ToString::to_string),
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
            "FROM rust:1.87 AS builder\nRUN cargo build\n\nFROM donbeave/jackin-construct:trixie AS runtime\nCOPY --from=builder /app /workspace/app\n",
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

        assert!(
            error
                .to_string()
                .contains("donbeave/jackin-construct:trixie")
        );
    }

    #[test]
    fn rejects_arg_indirection_in_final_from() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            "ARG BASE=donbeave/jackin-construct:trixie\nFROM ${BASE}\n",
        )
        .unwrap();

        let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("literal FROM donbeave/jackin-construct:trixie")
        );
    }
}
