//! Role repo validation: locate `jackin.role.toml`, clone/update, validate structure.
//!
//! Produces `ValidatedRoleRepo` once the manifest parses and the Dockerfile
//! passes `repo_contract` checks. Not responsible for git clone/update
//! mechanics (`runtime::repo_cache`) or manifest migration
//! (`manifest::migrations`).

use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_core::manifest::RoleManifest;

use crate::repo_contract::{MANIFEST_FILENAME, ValidatedDockerfile, validate_agent_dockerfile};
use std::path::{Component, Path, PathBuf};

/// On-disk cache location for a role repository clone.
#[derive(Debug, Clone)]
pub struct CachedRepo {
    /// Cache key: role selector key, or `key@branch` for branch-isolated entries.
    pub key: String,
    /// Absolute path to the cached clone directory.
    pub repo_dir: PathBuf,
}

impl CachedRepo {
    fn role_cache_root(paths: &JackinPaths, selector: &RoleSelector) -> PathBuf {
        selector.namespace.as_ref().map_or_else(
            || paths.roles_dir.join(&selector.name),
            |namespace| paths.roles_dir.join(namespace).join(&selector.name),
        )
    }

    /// Cache entry for the role's default-branch clone under `…/<role>/default`.
    pub fn new(paths: &JackinPaths, selector: &RoleSelector) -> Self {
        Self {
            key: selector.key(),
            repo_dir: Self::role_cache_root(paths, selector).join("default"),
        }
    }

    /// Cache directory isolated to a specific branch, leaving the default-branch
    /// cache entry untouched. The branch name is used directly as a path
    /// component so `feat/my-pr` lives at `…/branches/feat/my-pr` — a branch
    /// named `feat-my-pr` (with a dash) would live at `…/branches/feat-my-pr`,
    /// which is a different path, eliminating any ambiguity.
    pub fn for_branch(paths: &JackinPaths, selector: &RoleSelector, branch: &str) -> Self {
        Self {
            key: format!("{}@{branch}", selector.key()),
            // Path::join handles forward slashes as directory separators, so
            // "feat/my-pr" naturally becomes …/branches/feat/my-pr on disk.
            repo_dir: Self::role_cache_root(paths, selector)
                .join("branches")
                .join(branch),
        }
    }
}

/// Role repository that passed manifest + Dockerfile structural validation.
#[derive(Debug, Clone)]
pub struct ValidatedRoleRepo {
    /// Parsed role manifest from `jackin.role.toml`.
    pub manifest: RoleManifest,
    /// Dockerfile validated against the construct base-image contract.
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
    /// Required path is missing or not a regular file.
    #[error("missing {}", _0.display())]
    Missing(PathBuf),
    /// Manifest path field must be relative to the repo root.
    #[error("{label} path must be relative")]
    PathMustBeRelative {
        /// Human-readable field label (e.g. `"dockerfile"`, hook name).
        label: &'static str,
    },
    /// Path uses `..` or other components that leave the repo.
    #[error("{label} path must stay inside the repo")]
    PathOutsideRepo {
        /// Human-readable field label.
        label: &'static str,
    },
    /// Path resolves to a symlink; role paths must be real files.
    #[error("{label} path must not be a symlink")]
    PathIsSymlink {
        /// Human-readable field label.
        label: &'static str,
    },
    /// Canonicalized path escapes the repo root (symlink / mount tricks).
    #[error("{label} path escapes the repo boundary")]
    PathEscapesBoundary {
        /// Human-readable field label.
        label: &'static str,
    },
    /// Hook script exists but is zero-length.
    #[error("{label} is empty: {}", path.display())]
    EmptyHook {
        /// Human-readable hook label.
        label: &'static str,
        /// Absolute path of the empty hook file.
        path: PathBuf,
    },
    /// Dockerfile failed to parse.
    #[error("unable to parse Dockerfile: {0}")]
    DockerfileParse(String),
    /// Dockerfile has no `FROM` instruction.
    #[error("Dockerfile must contain at least one FROM instruction")]
    DockerfileMissingFrom,
    /// Final stage does not `FROM` the construct image (or sets a platform).
    #[error("final Dockerfile stage must use literal FROM {expected}")]
    DockerfileNonConstruct {
        /// Expected floating construct image reference for the error message.
        expected: String,
    },
    /// Final stage uses the floating construct tag instead of a version pin.
    #[error(
        "Dockerfile FROM {r}:{t} uses the floating tag — pin to a versioned release \
         like FROM {r}:<version>-{t}.\n\
         Configure Renovate with a regex versioning rule to track version bumps automatically.",
        r = crate::repo_contract::CONSTRUCT_REGISTRY_IMAGE,
        t = crate::repo_contract::CONSTRUCT_STABLE_TAG,
    )]
    DockerfileMissingVersionPin,
    /// Catch-all for non-structural failures (TOML parse, IO, manifest
    /// semantic validation). The friendly translator renders these as
    /// the generic "not a valid jackin❯ role" message.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<std::io::Error> for RoleRepoValidationError {
    fn from(err: std::io::Error) -> Self {
        Self::Other(err.into())
    }
}

/// Validate a role-repo directory: manifest present, Dockerfile contract,
/// relative non-symlink hooks, and semantic manifest rules.
///
/// # Errors
/// Returns [`RoleRepoValidationError`] for structural or semantic failures.
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
            // metadata().len() avoids slurping a potentially large hook
            // script into memory just to check emptiness.
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
mod tests;
