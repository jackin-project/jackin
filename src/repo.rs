use crate::manifest::RoleManifest;
use crate::paths::JackinPaths;
use crate::repo_contract::{ValidatedDockerfile, validate_agent_dockerfile};
use crate::selector::RoleSelector;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CachedRepo {
    pub key: String,
    pub repo_dir: PathBuf,
}

impl CachedRepo {
    pub fn new(paths: &JackinPaths, selector: &RoleSelector) -> Self {
        let repo_dir = selector.namespace.as_ref().map_or_else(
            || paths.roles_dir.join(&selector.name),
            |namespace| paths.roles_dir.join(namespace).join(&selector.name),
        );

        Self {
            key: selector.key(),
            repo_dir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedRoleRepo {
    pub manifest: RoleManifest,
    pub dockerfile: ValidatedDockerfile,
}

/// Specific structural rejections from `validate_role_repo`.
///
/// Variants carry the rejected path / label so the editor's friendly
/// translator can render rich messages without parsing free-form
/// strings; downstream `RepoError::InvalidRoleRepo` is matched against
/// the inner variant rather than substring-stripped.
///
/// Add a variant per new rejection rule; reach for `Other(anyhow::Error)`
/// only when the failure is a non-structural pass-through (manifest TOML
/// parse, IO error from the underlying filesystem, etc.).
#[derive(Debug, thiserror::Error)]
pub enum RoleRepoValidationError {
    #[error("missing {}", _0.display())]
    Missing(PathBuf),
    #[error("{label} path must be relative")]
    PathMustBeRelative { label: &'static str },
    #[error("{label} path must stay inside the repo")]
    PathOutsideRepo { label: &'static str },
    #[error("{label} path must not be a symlink")]
    PathIsSymlink { label: &'static str },
    #[error("{label} path escapes the repo boundary")]
    PathEscapesBoundary { label: &'static str },
    #[error("pre_launch hook is empty: {}", _0.display())]
    EmptyPreLaunchHook(PathBuf),
    #[error("unable to parse Dockerfile: {0}")]
    DockerfileParse(String),
    #[error("Dockerfile must contain at least one FROM instruction")]
    DockerfileMissingFrom,
    #[error("final Dockerfile stage must use literal FROM {expected}")]
    DockerfileNonConstruct { expected: &'static str },
    /// Catch-all for non-structural failures (TOML parse, IO, manifest
    /// semantic validation). The friendly translator renders these as
    /// the generic "not a valid Jackin role" message.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<std::io::Error> for RoleRepoValidationError {
    fn from(err: std::io::Error) -> Self {
        Self::Other(err.into())
    }
}

pub fn validate_role_repo(repo_dir: &Path) -> Result<ValidatedRoleRepo, RoleRepoValidationError> {
    let manifest_path = repo_dir.join("jackin.role.toml");

    if !manifest_path.is_file() {
        return Err(RoleRepoValidationError::Missing(manifest_path));
    }

    let manifest = RoleManifest::load(repo_dir)?;
    let dockerfile_path = validate_relative_path(repo_dir, &manifest.dockerfile, "dockerfile")?;
    let dockerfile = validate_agent_dockerfile(&dockerfile_path)?;

    if let Some(ref hooks) = manifest.hooks
        && let Some(ref pre_launch) = hooks.pre_launch
    {
        let hook_path = validate_relative_path(repo_dir, pre_launch, "pre_launch hook")?;
        let contents = std::fs::read_to_string(&hook_path)?;
        if contents.is_empty() {
            return Err(RoleRepoValidationError::EmptyPreLaunchHook(hook_path));
        }
    }

    let warnings = manifest.validate()?;
    for warning in &warnings {
        eprintln!("warning: {}", warning.message);
    }

    Ok(ValidatedRoleRepo {
        manifest,
        dockerfile,
    })
}

fn validate_relative_path(
    repo_dir: &Path,
    path_str: &str,
    label: &'static str,
) -> Result<PathBuf, RoleRepoValidationError> {
    let path = Path::new(path_str);

    if path.is_absolute() {
        return Err(RoleRepoValidationError::PathMustBeRelative { label });
    }

    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(RoleRepoValidationError::PathOutsideRepo { label });
        }
    }

    let resolved = repo_dir.join(path);
    if !resolved.is_file() {
        return Err(RoleRepoValidationError::Missing(resolved));
    }
    if std::fs::symlink_metadata(&resolved)?
        .file_type()
        .is_symlink()
    {
        return Err(RoleRepoValidationError::PathIsSymlink { label });
    }

    let canonical_repo = repo_dir.canonicalize()?;
    let canonical_resolved = resolved.canonicalize()?;
    if !canonical_resolved.starts_with(&canonical_repo) {
        return Err(RoleRepoValidationError::PathEscapesBoundary { label });
    }

    Ok(canonical_resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::RoleSelector;
    use tempfile::tempdir;

    #[test]
    fn computes_cached_repo_path_for_namespaced_selector() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(Some("chainargos"), "the-architect");

        let repo = CachedRepo::new(&paths, &selector);

        assert_eq!(
            repo.repo_dir,
            paths.roles_dir.join("chainargos").join("the-architect")
        );
    }

    #[test]
    fn rejects_repo_without_required_files() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path()).unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("jackin.role.toml"));
    }

    #[test]
    fn rejects_missing_manifest_dockerfile() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "docker/role.Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("docker/role.Dockerfile"));
    }

    #[test]
    fn rejects_manifest_dockerfile_outside_repo() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "../Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "escape/Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("escapes the repo boundary"));
    }

    #[test]
    fn accepts_manifest_dockerfile_in_subdirectory() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("docker")).unwrap();
        std::fs::write(
            temp.path().join("docker/role.Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "docker/role.Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let validated = validate_role_repo(temp.path()).unwrap();

        assert_eq!(
            validated.dockerfile.dockerfile_path,
            temp.path()
                .canonicalize()
                .unwrap()
                .join("docker/role.Dockerfile")
        );
    }

    #[test]
    fn accepts_manifest_with_valid_pre_launch_hook() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
        std::fs::write(
            temp.path().join("hooks/pre-launch.sh"),
            r"#!/bin/bash
echo hello
",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
        )
        .unwrap();

        let validated = validate_role_repo(temp.path()).unwrap();

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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "../escape.sh"
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/missing.sh"
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "/etc/evil.sh"
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
        )
        .unwrap();

        let error = validate_role_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("symlink"));
        assert!(error.to_string().contains("pre_launch"));
    }
}
