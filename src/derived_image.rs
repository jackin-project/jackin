use crate::repo::ValidatedRoleRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ENTRYPOINT_SH: &str = include_str!("../docker/runtime/entrypoint.sh");

#[derive(Debug)]
pub struct DerivedBuildContext {
    pub temp_dir: TempDir,
    pub context_dir: PathBuf,
    pub dockerfile_path: PathBuf,
}

pub fn render_derived_dockerfile(
    base_dockerfile: &str,
    pre_launch_hook: Option<&str>,
    supported: &[crate::agent::Agent],
) -> String {
    use crate::agent::profile::profile;

    let hook_section = pre_launch_hook.map_or_else(String::new, |hook_path| {
        format!(
            "\
USER root
COPY {hook_path} /home/agent/.jackin-runtime/pre-launch.sh
RUN chmod +x /home/agent/.jackin-runtime/pre-launch.sh
USER role
"
        )
    });

    // Concatenate per-agent install blocks. Claude, when present,
    // MUST come first so its ARG JACKIN_CACHE_BUST invalidates the
    // layer chain downstream into Codex's RUN. The slice's V1
    // invariant is "every role class supports Claude"; if that ever
    // changes, Codex's profile install_block will need its own
    // ARG JACKIN_CACHE_BUST line.
    let mut install_blocks = String::new();
    let mut sorted: Vec<crate::agent::Agent> = supported.to_vec();
    sorted.sort_by_key(|h| match h {
        crate::agent::Agent::Claude => 0,
        crate::agent::Agent::Codex => 1,
    });
    for h in sorted {
        install_blocks.push_str(&profile(h).install_block);
    }

    format!(
        "\
{base_dockerfile}
USER root
ARG JACKIN_HOST_UID=1000
ARG JACKIN_HOST_GID=1000
RUN current_gid=\"$(id -g role)\" \
    && current_uid=\"$(id -u role)\" \
    && if [ \"$current_gid\" != \"$JACKIN_HOST_GID\" ]; then \
         groupmod -o -g \"$JACKIN_HOST_GID\" role \
         && usermod -g \"$JACKIN_HOST_GID\" role; \
       fi \
    && if [ \"$current_uid\" != \"$JACKIN_HOST_UID\" ]; then \
         usermod -o -u \"$JACKIN_HOST_UID\" role; \
       fi \
    && chown -R role:role /home/agent
{install_blocks}{hook_section}USER root
COPY .jackin-runtime/entrypoint.sh /home/agent/entrypoint.sh
RUN chmod +x /home/agent/entrypoint.sh
USER role
ENTRYPOINT [\"/home/agent/entrypoint.sh\"]
"
    )
}

pub fn create_derived_build_context(
    repo_dir: &Path,
    validated: &ValidatedRoleRepo,
) -> anyhow::Result<DerivedBuildContext> {
    let temp_dir = tempfile::tempdir()?;
    let context_dir = temp_dir.path().join("context");
    copy_dir_all(repo_dir, &context_dir)?;

    let runtime_dir = context_dir.join(".jackin-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    std::fs::write(runtime_dir.join("entrypoint.sh"), ENTRYPOINT_SH)?;

    let pre_launch_hook = validated
        .manifest
        .hooks
        .as_ref()
        .and_then(|h| h.pre_launch.as_deref());

    let supported = validated.manifest.supported_agents();
    let dockerfile_path = context_dir.join(".jackin-runtime/DerivedDockerfile");
    std::fs::write(
        &dockerfile_path,
        render_derived_dockerfile(
            &validated.dockerfile.dockerfile_contents,
            pre_launch_hook,
            &supported,
        ),
    )?;
    ensure_runtime_assets_are_included(&context_dir, pre_launch_hook)?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

fn ensure_runtime_assets_are_included(
    context_dir: &Path,
    pre_launch_hook: Option<&str>,
) -> anyhow::Result<()> {
    let dockerignore_path = context_dir.join(".dockerignore");
    let mut dockerignore = if dockerignore_path.exists() {
        std::fs::read_to_string(&dockerignore_path)?
    } else {
        String::new()
    };

    let mut rules = vec![
        "!.jackin-runtime/".to_string(),
        "!.jackin-runtime/entrypoint.sh".to_string(),
        "!.jackin-runtime/DerivedDockerfile".to_string(),
    ];
    if let Some(hook_path) = pre_launch_hook {
        rules.push(format!("!{hook_path}"));
    }

    for rule in &rules {
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
                "invalid role repo: derived build context does not support symlinks: {}",
                entry.path().display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    fn renders_derived_dockerfile_with_workspace_and_entrypoint() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
        );

        assert!(dockerfile.contains("RUN curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(!dockerfile.contains("WORKDIR"));
        assert!(
            dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /home/agent/entrypoint.sh")
        );
        assert!(dockerfile.contains("ENTRYPOINT [\"/home/agent/entrypoint.sh\"]"));
    }

    #[test]
    fn renders_derived_dockerfile_installs_claude_as_agent_user() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
        );

        assert!(dockerfile.contains("USER role\n"));
        assert!(dockerfile.contains("ARG JACKIN_CACHE_BUST=0"));
        assert!(dockerfile.contains("RUN curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(dockerfile.contains("RUN claude --version"));
        assert!(
            dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /home/agent/entrypoint.sh")
        );
    }

    #[test]
    fn renders_derived_dockerfile_rewrites_agent_uid_and_gid() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
        );

        assert!(dockerfile.contains("ARG JACKIN_HOST_UID=1000"));
        assert!(dockerfile.contains("ARG JACKIN_HOST_GID=1000"));
        assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" role"));
        assert!(dockerfile.contains("usermod -g \"$JACKIN_HOST_GID\" role"));
        assert!(dockerfile.contains("usermod -o -u \"$JACKIN_HOST_UID\" role"));
    }

    #[test]
    fn renders_derived_dockerfile_with_pre_launch_hook() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            Some("hooks/pre-launch.sh"),
            &[Agent::Claude],
        );

        assert!(
            dockerfile
                .contains("COPY hooks/pre-launch.sh /home/agent/.jackin-runtime/pre-launch.sh")
        );
        assert!(dockerfile.contains("RUN chmod +x /home/agent/.jackin-runtime/pre-launch.sh"));
    }

    #[test]
    fn renders_derived_dockerfile_without_pre_launch_hook() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
        );

        assert!(!dockerfile.contains("pre-launch.sh"));
    }

    #[test]
    fn renders_dockerfile_with_codex_install_when_supported() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude, Agent::Codex],
        );

        assert!(dockerfile.contains("https://claude.ai/install.sh"));
        assert!(dockerfile.contains("openai/codex/releases"));
        // Claude block precedes Codex (cache-bust ordering).
        let claude_pos = dockerfile.find("claude.ai/install.sh").unwrap();
        let codex_pos = dockerfile.find("openai/codex/releases").unwrap();
        assert!(claude_pos < codex_pos);
    }

    #[test]
    fn renders_codex_only_dockerfile_without_claude_install() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Codex],
        );

        assert!(!dockerfile.contains("https://claude.ai/install.sh"));
        assert!(dockerfile.contains("openai/codex/releases"));
    }

    #[test]
    fn renders_dockerfile_targets_agent_user_not_claude() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
        );

        assert!(dockerfile.contains("/home/agent"));
        assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" role"));
        assert!(dockerfile.contains("ENTRYPOINT [\"/home/agent/entrypoint.sh\"]"));
        assert!(!dockerfile.contains("/home/claude"));
    }

    #[test]
    fn renders_dockerfile_does_not_set_jackin_harness_env() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude, Agent::Codex],
        );

        assert!(!dockerfile.contains("ENV JACKIN_AGENT"));
    }

    #[test]
    fn entrypoint_does_not_override_claude_env() {
        assert!(!ENTRYPOINT_SH.contains("JACKIN="));
    }

    #[test]
    fn entrypoint_dispatches_on_jackin_harness() {
        assert!(ENTRYPOINT_SH.contains("case \"${JACKIN_AGENT:?"));
        assert!(ENTRYPOINT_SH.contains("  claude)"));
        assert!(ENTRYPOINT_SH.contains("  codex)"));
    }

    #[test]
    fn entrypoint_claude_branch_invokes_install_claude_plugins() {
        assert!(ENTRYPOINT_SH.contains("/home/agent/install-claude-plugins.sh"));
    }

    #[test]
    fn entrypoint_codex_branch_does_not_invoke_install_claude_plugins() {
        let codex_section = ENTRYPOINT_SH
            .split("codex)")
            .nth(1)
            .unwrap()
            .split(";;")
            .next()
            .unwrap();
        assert!(!codex_section.contains("install-claude-plugins.sh"));
    }

    #[test]
    fn entrypoint_registers_security_tool_mcp_servers() {
        let claude_section = ENTRYPOINT_SH
            .split("claude)")
            .nth(1)
            .unwrap()
            .split(";;")
            .next()
            .unwrap();
        assert!(claude_section.contains("claude mcp add tirith -- tirith mcp-server"));
        assert!(claude_section.contains("claude mcp add shellfirm -- shellfirm mcp"));
    }

    #[test]
    fn entrypoint_mcp_registration_respects_disable_guards() {
        assert!(ENTRYPOINT_SH.contains("JACKIN_DISABLE_TIRITH"));
        assert!(ENTRYPOINT_SH.contains("JACKIN_DISABLE_SHELLFIRM"));
    }

    #[test]
    fn entrypoint_pre_launch_hook_path_uses_agent_home() {
        assert!(ENTRYPOINT_SH.contains("/home/agent/.jackin-runtime/pre-launch.sh"));
        assert!(!ENTRYPOINT_SH.contains("/home/claude"));
    }

    #[test]
    fn creates_temp_context_with_repo_copy_and_runtime_assets() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join(".dockerignore"),
            r".*
.jackin-runtime
",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
        std::fs::write(repo.path().join("shared.txt"), "hello\n").unwrap();
        symlink(
            repo.path().join("shared.txt"),
            repo.path().join("linked.txt"),
        )
        .unwrap();

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let error = create_derived_build_context(repo.path(), &validated)
            .expect_err("symlinks should be rejected");

        assert!(error.to_string().contains("symlink"));
        assert!(error.to_string().contains("linked.txt"));
    }
}
