use crate::manifest::AgentManifest;
use crate::paths::JackinPaths;
use crate::repo_contract::{ValidatedDockerfile, validate_agent_dockerfile};
use crate::selector::ClassSelector;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CachedRepo {
    pub key: String,
    pub repo_dir: PathBuf,
}

impl CachedRepo {
    pub fn new(paths: &JackinPaths, selector: &ClassSelector) -> Self {
        let repo_dir = selector.namespace.as_ref().map_or_else(
            || paths.agents_dir.join(&selector.name),
            |namespace| paths.agents_dir.join(namespace).join(&selector.name),
        );

        Self {
            key: selector.key(),
            repo_dir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedAgentRepo {
    pub manifest: AgentManifest,
    pub dockerfile: ValidatedDockerfile,
}

pub fn validate_agent_repo(repo_dir: &Path) -> anyhow::Result<ValidatedAgentRepo> {
    let manifest_path = repo_dir.join("jackin.agent.toml");

    if !manifest_path.is_file() {
        anyhow::bail!("invalid agent repo: missing {}", manifest_path.display());
    }

    let manifest = AgentManifest::load(repo_dir)?;
    let dockerfile_path = resolve_manifest_dockerfile_path(repo_dir, &manifest)?;
    let dockerfile = validate_agent_dockerfile(&dockerfile_path)?;

    // Validate pre-launch hook path if declared
    if let Some(ref hooks) = manifest.hooks
        && let Some(ref pre_launch) = hooks.pre_launch
    {
        let hook_path = validate_relative_path(repo_dir, pre_launch, "pre_launch hook")?;
        let contents = std::fs::read_to_string(&hook_path)?;
        if contents.is_empty() {
            anyhow::bail!(
                "invalid agent repo: pre_launch hook is empty: {}",
                hook_path.display()
            );
        }
    }

    // Validate env var declarations
    let warnings = manifest.validate()?;
    for warning in &warnings {
        eprintln!("warning: {}", warning.message);
    }

    Ok(ValidatedAgentRepo {
        manifest,
        dockerfile,
    })
}

fn validate_relative_path(repo_dir: &Path, path_str: &str, label: &str) -> anyhow::Result<PathBuf> {
    let path = Path::new(path_str);

    if path.is_absolute() {
        anyhow::bail!("invalid agent repo: {label} path must be relative");
    }

    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            anyhow::bail!("invalid agent repo: {label} path must stay inside the repo");
        }
    }

    let resolved = repo_dir.join(path);
    if !resolved.is_file() {
        anyhow::bail!("invalid agent repo: missing {}", resolved.display());
    }
    if std::fs::symlink_metadata(&resolved)?
        .file_type()
        .is_symlink()
    {
        anyhow::bail!("invalid agent repo: {label} path must not be a symlink");
    }

    let canonical_repo = repo_dir.canonicalize()?;
    let canonical_resolved = resolved.canonicalize()?;
    if !canonical_resolved.starts_with(&canonical_repo) {
        anyhow::bail!("invalid agent repo: {label} path escapes the repo boundary");
    }

    Ok(canonical_resolved)
}

fn resolve_manifest_dockerfile_path(
    repo_dir: &Path,
    manifest: &AgentManifest,
) -> anyhow::Result<PathBuf> {
    validate_relative_path(repo_dir, &manifest.dockerfile, "dockerfile")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn computes_cached_repo_path_for_namespaced_selector() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(Some("chainargos"), "the-architect");

        let repo = CachedRepo::new(&paths, &selector);

        assert_eq!(
            repo.repo_dir,
            paths.agents_dir.join("chainargos").join("the-architect")
        );
    }

    #[test]
    fn rejects_repo_without_required_files() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path()).unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("jackin.agent.toml"));
    }

    #[test]
    fn rejects_missing_manifest_dockerfile() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "docker/agent.Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("docker/agent.Dockerfile"));
    }

    #[test]
    fn rejects_manifest_dockerfile_outside_repo() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "../Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("must stay inside the repo"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_repo_boundary() {
        let temp = tempdir().unwrap();
        let outside = tempdir().unwrap();
        std::fs::write(outside.path().join("Dockerfile"), "FROM debian:trixie\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "escape/Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("escapes the repo boundary"));
    }

    #[test]
    fn accepts_manifest_dockerfile_in_subdirectory() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("docker")).unwrap();
        std::fs::write(
            temp.path().join("docker/agent.Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "docker/agent.Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let validated = validate_agent_repo(temp.path()).unwrap();

        assert_eq!(
            validated.dockerfile.dockerfile_path,
            temp.path()
                .canonicalize()
                .unwrap()
                .join("docker/agent.Dockerfile")
        );
    }

    #[test]
    fn accepts_manifest_with_valid_pre_launch_hook() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
        std::fs::write(
            temp.path().join("hooks/pre-launch.sh"),
            r#"#!/bin/bash
echo hello
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
        )
        .unwrap();

        let validated = validate_agent_repo(temp.path()).unwrap();

        assert!(
            validated
                .manifest
                .hooks
                .as_ref()
                .unwrap()
                .pre_launch
                .is_some()
        );
    }

    #[test]
    fn rejects_pre_launch_hook_outside_repo() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "../escape.sh"
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("must stay inside the repo"));
    }

    #[test]
    fn rejects_pre_launch_hook_that_does_not_exist() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/missing.sh"
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn rejects_absolute_pre_launch_hook_path() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "/etc/evil.sh"
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("must be relative"));
    }

    #[test]
    fn rejects_empty_pre_launch_hook() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
        std::fs::write(temp.path().join("hooks/pre-launch.sh"), "").unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("empty"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_pre_launch_hook_inside_repo() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
        std::fs::write(temp.path().join("real-hook.sh"), "#!/bin/bash\necho hi\n").unwrap();
        std::os::unix::fs::symlink(
            temp.path().join("real-hook.sh"),
            temp.path().join("hooks/pre-launch.sh"),
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("symlink"));
        assert!(error.to_string().contains("pre_launch"));
    }
}
