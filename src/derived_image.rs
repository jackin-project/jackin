use crate::repo::ValidatedAgentRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ENTRYPOINT_SH: &str = include_str!("../docker/runtime/entrypoint.sh");

pub struct DerivedBuildContext {
    pub temp_dir: TempDir,
    pub context_dir: PathBuf,
    pub dockerfile_path: PathBuf,
}

pub fn render_derived_dockerfile(base_dockerfile: &str) -> String {
    format!(
        "{base_dockerfile}\nUSER root\nRUN curl -fsSL https://claude.ai/install.sh | bash\nRUN claude --version\nCOPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh\nRUN chmod +x /home/claude/entrypoint.sh\nWORKDIR /workspace\nUSER claude\nENTRYPOINT [\"/home/claude/entrypoint.sh\"]\n"
    )
}

pub fn create_derived_build_context(
    repo_dir: &Path,
    validated: &ValidatedAgentRepo,
) -> anyhow::Result<DerivedBuildContext> {
    let temp_dir = tempfile::tempdir()?;
    let context_dir = temp_dir.path().join("context");
    copy_dir_all(repo_dir, &context_dir)?;

    let runtime_dir = context_dir.join(".jackin-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    std::fs::write(runtime_dir.join("entrypoint.sh"), ENTRYPOINT_SH)?;

    let dockerfile_path = context_dir.join(".jackin-runtime/DerivedDockerfile");
    std::fs::write(
        &dockerfile_path,
        render_derived_dockerfile(&validated.dockerfile.dockerfile_contents),
    )?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

fn copy_dir_all(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = to.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn renders_derived_dockerfile_with_workspace_and_entrypoint() {
        let dockerfile = render_derived_dockerfile("FROM jackin/construct:trixie\n");

        assert!(dockerfile.contains("RUN curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(dockerfile.contains("WORKDIR /workspace"));
        assert!(
            dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh")
        );
        assert!(dockerfile.contains("ENTRYPOINT [\"/home/claude/entrypoint.sh\"]"));
    }

    #[test]
    fn creates_temp_context_with_repo_copy_and_runtime_assets() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM jackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let validated = crate::repo::validate_agent_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated).unwrap();

        assert!(build.context_dir.join("Dockerfile").is_file());
        assert!(
            build
                .context_dir
                .join(".jackin-runtime/entrypoint.sh")
                .is_file()
        );
        assert!(build.dockerfile_path.is_file());
    }
}
