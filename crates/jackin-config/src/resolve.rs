//! Workspace resolution: build `ResolvedWorkspace` from a saved or current-directory workspace.
//!
//! Handles both `LoadWorkspaceInput::Saved` and `LoadWorkspaceInput::CurrentDir`
//! paths through the same validation pipeline. Not responsible for mount
//! parsing from CLI strings (`workspace::mounts`) or sensitive-path detection
//! (`workspace::sensitive`).

use std::path::{Path, PathBuf};

use jackin_core::{MountIsolation, RoleSelector};

use crate::app_config::AppConfig;
use crate::paths::expand_tilde;
use crate::schema::{MountConfig, ResolvedWorkspace, WorkspaceConfig, validate_mount_paths};
use jackin_core::WorkspaceName;
use crate::validation::validate_workspace_config;

pub fn current_dir_workspace(cwd: &Path) -> anyhow::Result<WorkspaceConfig> {
    let cwd = cwd.canonicalize()?;
    let path = cwd.display().to_string();

    Ok(WorkspaceConfig {
        workdir: path.clone(),
        mounts: vec![MountConfig {
            src: path.clone(),
            dst: path,
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..Default::default()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWorkspaceInput {
    CurrentDir,
    Path { src: String, dst: String },
    Saved(String),
}

fn host_path_match_depth(path: &str, canonical_cwd: &Path) -> Option<usize> {
    let expanded = expand_tilde(path);
    let canonical_path = Path::new(&expanded).canonicalize().ok()?;

    if canonical_cwd == canonical_path || canonical_cwd.starts_with(&canonical_path) {
        Some(canonical_path.components().count())
    } else {
        None
    }
}

pub fn saved_workspace_match_depth(workspace: &WorkspaceConfig, cwd: &Path) -> Option<usize> {
    let canonical_cwd = cwd.canonicalize().ok()?;

    // Workdir must match exactly — being a parent of cwd is not enough.
    // Mount sources still match as a prefix so that subdirectories of a
    // mounted host path are covered without needing to enumerate every file.
    let workdir_depth = {
        let expanded = expand_tilde(&workspace.workdir);
        Path::new(&expanded)
            .canonicalize()
            .ok()
            .filter(|p| &canonical_cwd == p)
            .map(|p| p.components().count())
    };

    std::iter::once(workdir_depth)
        .chain(
            workspace
                .mounts
                .iter()
                .map(|mount| host_path_match_depth(&mount.src, &canonical_cwd)),
        )
        .flatten()
        .max()
}

pub fn resolve_load_workspace(
    config: &AppConfig,
    selector: &RoleSelector,
    cwd: &Path,
    input: LoadWorkspaceInput,
    ad_hoc_mounts: &[MountConfig],
) -> anyhow::Result<ResolvedWorkspace> {
    // Note on `keep_awake`: only `Saved` workspaces can opt in.
    // `CurrentDir` and `Path` build a fresh `WorkspaceConfig` from
    // defaults (`enabled = false`), so an ad-hoc load against a
    // directory that *would* match a saved keep-awake workspace
    // intentionally does not inherit the assertion — the user opted
    // in for the saved workspace, not for arbitrary loads.
    let (mut workspace, label) = match input {
        LoadWorkspaceInput::CurrentDir => {
            let ws = current_dir_workspace(cwd)?;
            let label = ws.workdir.clone();
            (ws, label)
        }
        LoadWorkspaceInput::Path { src, dst } => {
            let expanded_src = expand_tilde(&src);
            let abs_src = if Path::new(&expanded_src).is_absolute() {
                PathBuf::from(&expanded_src)
            } else {
                cwd.join(&expanded_src)
            };
            let canonical_src = abs_src
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("cannot resolve path {expanded_src}: {e}"))?;
            let src_str = canonical_src.display().to_string();
            let workdir = if dst == src || dst == expanded_src {
                src_str.clone()
            } else {
                dst.clone()
            };
            let ws = WorkspaceConfig {
                workdir,
                mounts: vec![MountConfig {
                    src: src_str,
                    dst: if dst == src || dst == expanded_src {
                        canonical_src.display().to_string()
                    } else {
                        dst
                    },
                    readonly: false,
                    isolation: MountIsolation::Shared,
                }],
                ..Default::default()
            };
            let label = ws.workdir.clone();
            (ws, label)
        }
        LoadWorkspaceInput::Saved(name) => {
            let workspace = config.require_workspace(&name)?.clone();
            if !workspace.allowed_roles.is_empty()
                && !workspace
                    .allowed_roles
                    .iter()
                    .any(|role| role == &selector.key())
            {
                anyhow::bail!("role {} is not allowed by workspace {name}", selector.key());
            }
            (workspace, name)
        }
    };

    // Merge ad-hoc mounts after workspace mounts, checking for dst conflicts.
    for ad_hoc in ad_hoc_mounts {
        if workspace
            .mounts
            .iter()
            .any(|existing| existing.dst == ad_hoc.dst)
        {
            anyhow::bail!(
                "ad-hoc mount destination conflicts with workspace mount destination: {}",
                ad_hoc.dst
            );
        }
        workspace.mounts.push(ad_hoc.clone());
    }

    validate_workspace_config(&WorkspaceName::parse("runtime").map_err(anyhow::Error::from)?, &workspace)?;
    validate_mount_paths(&workspace.mounts)?;

    let mut mounts = workspace.mounts.clone();
    let global_rows = config.resolve_mount_rows(selector);
    AppConfig::validate_effective_mount_destinations(&workspace, &global_rows)?;
    let global_mounts: Vec<(String, MountConfig)> = global_rows
        .into_iter()
        .map(|row| (row.name, row.mount))
        .collect();
    let global_mounts = AppConfig::expand_and_validate_named_mounts(&global_mounts)?;

    for mount in global_mounts {
        if mounts.iter().any(|existing| existing.dst == mount.dst) {
            anyhow::bail!(
                "global mount destination conflicts with workspace destination: {}",
                mount.dst
            );
        }
        mounts.push(mount);
    }

    Ok(ResolvedWorkspace {
        name: label.clone(),
        label,
        workdir: workspace.workdir,
        mounts,
        keep_awake_enabled: workspace.keep_awake.enabled,
        default_agent: workspace.default_agent,
        git_pull_on_entry: workspace.git_pull_on_entry,
    })
}

/// Find the saved workspace that best matches the current working directory.
///
/// Workspace workdirs must match `cwd` exactly; mount sources match as a
/// prefix. When multiple workspaces match, the deepest path wins (most
/// specific mount point).
///
/// Used by CLI context resolution and the console's workspace preselection.
pub fn find_saved_workspace_for_cwd<'a>(
    config: &'a AppConfig,
    cwd: &Path,
) -> Option<(&'a str, &'a WorkspaceConfig)> {
    config
        .workspaces
        .iter()
        .filter_map(|(name, ws)| {
            saved_workspace_match_depth(ws, cwd).map(|depth| (name, ws, depth))
        })
        .max_by_key(|(_, _, depth)| *depth)
        .map(|(name, ws, _)| (name.as_str(), ws))
}

#[cfg(test)]
mod tests;
