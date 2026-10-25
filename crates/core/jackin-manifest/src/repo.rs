// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::JackinPaths;
use jackin_core::RoleManifest;
use jackin_core::RoleSelector;

use crate::repo_contract::{MANIFEST_FILENAME, ValidatedDockerfile, validate_agent_dockerfile};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CachedRepo {
    pub key: String,
    pub repo_dir: PathBuf,
}

impl CachedRepo {
    fn role_cache_root(paths: &JackinPaths, selector: &RoleSelector) -> PathBuf {
        selector.namespace.as_ref().map_or_else(
            || paths.roles_dir.join(&selector.name),
            |namespace| paths.roles_dir.join(namespace).join(&selector.name),
        )
    }

    pub fn new(paths: &JackinPaths, selector: &RoleSelector) -> Self {
        Self {
            key: selector.key(),
            repo_dir: Self::role_cache_root(paths, selector).join("default"),
        }
    }

    pub fn for_branch(paths: &JackinPaths, selector: &RoleSelector, branch: &str) -> Self {
        Self {
            key: format!("{}@{branch}", selector.key()),
            repo_dir: Self::role_cache_root(paths, selector)
                .join("branches")
                .join(branch),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedRoleRepo {
    pub manifest: RoleManifest,
    pub dockerfile: ValidatedDockerfile,
}

#[derive(Debug, thiserror::Error)]
pub enum RoleRepoValidationError {
    #[error("missing {}", _0.display())]
    Missing(PathBuf),
    #[error("{label} path must be relative")]
    PathMustBeRelative { label: &'static str },

    #[error("{label} path must stay inside the repo")]
    PathOutsideRepo { label: &'static str },

    #[error("{label} path escapes the repo boundary")]
    PathEscapesBoundary { label: &'static str },

    #[error("{label} is empty: {}", path.display())]
    EmptyHook { label: &'static str, path: PathBuf },

    #[error("unable to parse Dockerfile: {0}")]
    DockerfileParse(String),
    #[error("Dockerfile must contain at least one FROM instruction")]
    DockerfileMissingFrom,
    #[error("final Dockerfile stage must use literal FROM {expected}")]
    DockerfileNonConstruct { expected: String },

    #[error(
        "Dockerfile FROM {r}:{t} uses the floating tag — pin to a versioned release \
         like FROM {r}:<version>-{t}.\n\
         Configure Renovate with a regex versioning rule to track version bumps automatically.",
        r = crate::repo_contract::CONSTRUCT_REGISTRY_IMAGE,
        t = crate::repo_contract::CONSTRUCT_STABLE_TAG,
    )]
    DockerfileMissingVersionPin,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<std::io::Error> for RoleRepoValidationError {
    fn from(err: std::io::Error) -> Self {
        Self::Other(err.into())
    }
}

pub fn validate_role_repo(repo_dir: &Path) -> Result<ValidatedRoleRepo, RoleRepoValidationError> {
    let manifest_path = repo_dir.join(MANIFEST_FILENAME);

    if !manifest_path.is_file() {
        return Err(RoleRepoValidationError::Missing(manifest_path));
    }

    let manifest = crate::manifest::load_role_manifest(repo_dir)?;
    let dockerfile_path = validate_relative_path(repo_dir, &manifest.dockerfile, "dockerfile")?;
    let dockerfile = validate_agent_dockerfile(&dockerfile_path)?;

    if let Some(ref hooks) = manifest.hooks {
        for entry in hooks.entries() {
            let hook_path = validate_relative_path(repo_dir, entry.path, entry.label)?;
            if std::fs::metadata(&hook_path)?.len() == 0 {
                return Err(RoleRepoValidationError::EmptyHook {
                    label: entry.label,
                    path: hook_path,
                });
            }
        }
    }

    let warnings = crate::validate::validate_role_manifest(&manifest)?;
    for warning in &warnings {
        jackin_core::emit_compact_line("warning", &format!("warning: {}", warning.message));
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

    let canonical_repo = repo_dir.canonicalize()?;
    let canonical_resolved = resolved.canonicalize()?;
    if !canonical_resolved.starts_with(&canonical_repo) {
        return Err(RoleRepoValidationError::PathEscapesBoundary { label });
    }

    Ok(canonical_resolved)
}
