// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ConfigError;
use std::path::{Path, PathBuf};

use jackin_core::{MountIsolation, RoleSelector};

use crate::app_config::AppConfig;
use crate::paths::expand_tilde;
use crate::schema::{
    MountConfig, ResolvedWorkspace, WorkspaceConfig, ensure_mount_sources, launch_cache_roots,
    validate_mount_paths,
};
use crate::validation::validate_workspace_config;
use jackin_core::WorkspaceName;

pub fn current_dir_workspace(cwd: &Path) -> crate::ConfigResult<WorkspaceConfig> {
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
) -> crate::ConfigResult<ResolvedWorkspace> {
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
            let canonical_src = abs_src.canonicalize().map_err(|e| {
                anyhow::Error::from(ConfigError::msg(format_args!(
                    "cannot resolve path {expanded_src}: {e}"
                )))
            })?;
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
            let wn = WorkspaceName::parse(&name).map_err(anyhow::Error::from)?;
            let workspace = config.require_workspace(&wn)?.clone();
            if !workspace.allowed_roles.is_empty()
                && !workspace
                    .allowed_roles
                    .iter()
                    .any(|role| role == &selector.key())
            {
                return Err(ConfigError::msg(format_args!(
                    "role {} is not allowed by workspace {name}",
                    selector.key()
                )));
            }
            (workspace, name)
        }
    };

    for ad_hoc in ad_hoc_mounts {
        if workspace
            .mounts
            .iter()
            .any(|existing| existing.dst == ad_hoc.dst)
        {
            return Err(ConfigError::msg(format_args!(
                "ad-hoc mount destination conflicts with workspace mount destination: {}",
                ad_hoc.dst
            )));
        }
        workspace.mounts.push(ad_hoc.clone());
    }

    validate_workspace_config(
        &WorkspaceName::parse("runtime").map_err(anyhow::Error::from)?,
        &workspace,
    )?;

    let cache_roots = launch_cache_roots();
    let owned_mounts = std::mem::take(&mut workspace.mounts);
    let (healed_mounts, mut heal_report) = ensure_mount_sources(
        owned_mounts
            .into_iter()
            .map(|mount| (None, mount))
            .collect(),
        &cache_roots,
    );
    workspace.mounts = healed_mounts;
    validate_mount_paths(&workspace.mounts)?;

    let mut mounts = workspace.mounts.clone();
    let global_rows = config.resolve_mount_rows(selector);
    AppConfig::validate_effective_mount_destinations(&workspace, &global_rows)?;
    let global_mounts: Vec<(String, MountConfig)> = global_rows
        .into_iter()
        .map(|row| (row.name, row.mount))
        .collect();
    let (global_mounts, global_report) =
        AppConfig::expand_and_validate_named_mounts(&global_mounts)?;
    heal_report.merge(global_report);

    for mount in global_mounts {
        if mounts.iter().any(|existing| existing.dst == mount.dst) {
            return Err(ConfigError::msg(format_args!(
                "global mount destination conflicts with workspace destination: {}",
                mount.dst
            )));
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
        mount_heal: heal_report,
    })
}

impl AppConfig {
    #[must_use]
    pub fn effective_default_launch(
        &self,
        workspace: Option<&WorkspaceName>,
        role: &str,
    ) -> Option<&[String]> {
        let ws = match workspace {
            Some(name) => Some(self.workspaces.get(name.as_str())?),
            None => None,
        };
        ws.and_then(|w| w.roles.get(role))
            .and_then(|r| r.default_launch.as_deref())
            .or_else(|| ws.and_then(|w| w.default_launch.as_deref()))
            .or(self.default_launch.as_deref())
    }
}

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
