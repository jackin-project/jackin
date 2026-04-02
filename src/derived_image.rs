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
        "{base_dockerfile}\nUSER root\nARG JACKIN_HOST_UID=1000\nARG JACKIN_HOST_GID=1000\nRUN current_gid=\"$(id -g claude)\" && current_uid=\"$(id -u claude)\" && if [ \"$current_gid\" != \"$JACKIN_HOST_GID\" ]; then groupmod -o -g \"$JACKIN_HOST_GID\" claude && usermod -g \"$JACKIN_HOST_GID\" claude; fi && if [ \"$current_uid\" != \"$JACKIN_HOST_UID\" ]; then usermod -o -u \"$JACKIN_HOST_UID\" claude; fi && chown -R claude:claude /home/claude\nUSER claude\nRUN curl -fsSL https://claude.ai/install.sh | bash\nRUN claude --version\nUSER root\nCOPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh\nRUN chmod +x /home/claude/entrypoint.sh\nWORKDIR /workspace\nUSER claude\nENTRYPOINT [\"/home/claude/entrypoint.sh\"]\n"
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
    ensure_runtime_assets_are_included(&context_dir)?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

fn ensure_runtime_assets_are_included(context_dir: &Path) -> anyhow::Result<()> {
    let dockerignore_path = context_dir.join(".dockerignore");
    let mut dockerignore = if dockerignore_path.exists() {
        std::fs::read_to_string(&dockerignore_path)?
    } else {
        String::new()
    };

    for rule in [
        "!.jackin-runtime/",
        "!.jackin-runtime/entrypoint.sh",
        "!.jackin-runtime/DerivedDockerfile",
    ] {
        if !dockerignore.lines().any(|line| line == rule) {
            if !dockerignore.is_empty() && !dockerignore.ends_with('\n') {
                dockerignore.push('\n');
            }
            dockerignore.push_str(rule);
            dockerignore.push('\n');
        }
    }

    std::fs::write(dockerignore_path, dockerignore)?;
    Ok(())
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
        } else if file_type.is_symlink() {
            anyhow::bail!(
                "invalid agent repo: derived build context does not support symlinks: {}",
                entry.path().display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    fn renders_derived_dockerfile_with_workspace_and_entrypoint() {
        let dockerfile = render_derived_dockerfile("FROM donbeave/jackin-construct:trixie\n");

        assert!(dockerfile.contains("RUN curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(dockerfile.contains("WORKDIR /workspace"));
        assert!(
            dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh")
        );
        assert!(dockerfile.contains("ENTRYPOINT [\"/home/claude/entrypoint.sh\"]"));
    }

    #[test]
    fn renders_derived_dockerfile_installs_claude_as_claude_user() {
        let dockerfile = render_derived_dockerfile("FROM donbeave/jackin-construct:trixie\n");
        let install =
            "USER claude\nRUN curl -fsSL https://claude.ai/install.sh | bash\nRUN claude --version";
        let copy = "USER root\nCOPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh";

        assert!(dockerfile.contains(install));
        assert!(dockerfile.contains(copy));
    }

    #[test]
    fn renders_derived_dockerfile_rewrites_claude_uid_and_gid() {
        let dockerfile = render_derived_dockerfile("FROM donbeave/jackin-construct:trixie\n");

        assert!(dockerfile.contains("ARG JACKIN_HOST_UID=1000"));
        assert!(dockerfile.contains("ARG JACKIN_HOST_GID=1000"));
        assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" claude"));
        assert!(dockerfile.contains("usermod -g \"$JACKIN_HOST_GID\" claude"));
        assert!(dockerfile.contains("usermod -o -u \"$JACKIN_HOST_UID\" claude"));
    }

    #[test]
    fn creates_temp_context_with_repo_copy_and_runtime_assets() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
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

    #[test]
    fn preserves_runtime_assets_when_repo_dockerignore_excludes_hidden_paths() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(repo.path().join(".dockerignore"), ".*\n.jackin-runtime\n").unwrap();
        std::fs::write(
            repo.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let validated = crate::repo::validate_agent_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated).unwrap();
        let dockerignore =
            std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

        assert!(dockerignore.contains("!.jackin-runtime/"));
        assert!(dockerignore.contains("!.jackin-runtime/entrypoint.sh"));
        assert!(dockerignore.contains("!.jackin-runtime/DerivedDockerfile"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_repo_build_context() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();
        std::fs::write(repo.path().join("shared.txt"), "hello\n").unwrap();
        symlink(
            repo.path().join("shared.txt"),
            repo.path().join("linked.txt"),
        )
        .unwrap();

        let validated = crate::repo::validate_agent_repo(repo.path()).unwrap();
        let error = create_derived_build_context(repo.path(), &validated)
            .err()
            .expect("symlinks should be rejected");

        assert!(error.to_string().contains("symlink"));
        assert!(error.to_string().contains("linked.txt"));
    }
}
