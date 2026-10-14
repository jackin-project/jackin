// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::MountIsolation;
use crate::branch::branch_name;
use crate::error::IsolationError;
use crate::state::{
    CleanupStatus, IsolationRecord, read_record, read_worktree_cleanup, upsert_record,
    worktree_cleanup_journal_name,
};
use anyhow::Context;
use jackin_config::ResolvedWorkspace;
use jackin_core::CommandRunner;
use jackin_core::WorkspaceLabel;
use jackin_core::container_paths;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedWorkspace {
    pub workdir: String,
    pub mounts: Vec<MaterializedMount>,

    pub keep_awake_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedMount {
    pub bind_src: String,
    pub dst: String,
    pub readonly: bool,
    pub isolation: MountIsolation,

    pub worktree_aux: Option<WorktreeAuxMounts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeAuxMounts {
    pub host_git_dir: String,
    pub host_git_target: String,

    pub git_file_override: String,
    pub git_file_target: String,

    pub gitdir_back_override: String,
    pub gitdir_back_target: String,
}

pub fn worktree_path_for(container_state_dir: &Path, dst: &str, container_name: &str) -> PathBuf {
    let rel = dst.trim_matches('/');
    container_state_dir
        .join("git")
        .join("worktree")
        .join("repo")
        .join(rel)
        .join(container_name)
}

pub fn clone_path_for(container_state_dir: &Path, dst: &str, container_name: &str) -> PathBuf {
    let rel = dst.trim_matches('/');
    container_state_dir
        .join("git")
        .join("clone")
        .join("repo")
        .join(rel)
        .join(container_name)
}

fn container_host_git_path(mount_dst: &str) -> String {
    let rel = mount_dst.trim_matches('/');
    format!("{}/{rel}/.git", container_paths::HOST_DIR)
}

fn normalize_github_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return format!("https://github.com/{rest}");
    }
    if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        return format!("https://github.com/{rest}");
    }
    url.to_owned()
}

fn strip_userinfo(url: String) -> String {
    for scheme in ["https://", "http://"] {
        if let Some(rest) = url.strip_prefix(scheme) {
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            if let Some((_userinfo, host)) = authority.rsplit_once('@') {
                return if path.is_empty() {
                    format!("{scheme}{host}")
                } else {
                    format!("{scheme}{host}/{path}")
                };
            }
            return url;
        }
    }
    url
}

fn write_git_overrides(
    container_state_dir: &Path,
    mount_dst: &str,
    container_name: &str,
    host_repo_src: &str,
) -> anyhow::Result<WorktreeAuxMounts> {
    let rel = mount_dst.trim_matches('/');
    let mount_overrides_dir = container_state_dir.join("git").join("overrides").join(rel);
    std::fs::create_dir_all(&mount_overrides_dir)
        .with_context(|| format!("create overrides dir at {}", mount_overrides_dir.display()))?;

    let host_git_target = container_host_git_path(mount_dst);

    let git_file_override_path = mount_overrides_dir.join(".git");
    let git_file_content = format!("gitdir: {host_git_target}/worktrees/{container_name}\n");
    std::fs::write(&git_file_override_path, &git_file_content)
        .with_context(|| format!("write .git override {}", git_file_override_path.display()))?;

    let gitdir_back_override_path = mount_overrides_dir.join("gitdir");
    let gitdir_back_content = format!("{mount_dst}/.git\n");
    std::fs::write(&gitdir_back_override_path, &gitdir_back_content).with_context(|| {
        format!(
            "write gitdir override {}",
            gitdir_back_override_path.display()
        )
    })?;

    let host_git_dir = format!("{host_repo_src}/.git");
    let git_file_target = format!("{mount_dst}/.git");
    let gitdir_back_target = format!("{host_git_target}/worktrees/{container_name}/gitdir");

    Ok(WorktreeAuxMounts {
        host_git_dir,
        host_git_target,
        git_file_override: git_file_override_path.to_string_lossy().into(),
        git_file_target,
        gitdir_back_override: gitdir_back_override_path.to_string_lossy().into(),
        gitdir_back_target,
    })
}

pub async fn ensure_worktree_config_enabled(
    repo: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<bool> {
    let current = runner
        .capture(
            "git",
            &[
                "-C",
                &repo.to_string_lossy(),
                "config",
                "--get",
                "extensions.worktreeConfig",
            ],
            None,
        )
        .await
        .unwrap_or_default();
    if current.trim() == "true" {
        return Ok(false);
    }
    let format_version = runner
        .capture(
            "git",
            &[
                "-C",
                &repo.to_string_lossy(),
                "config",
                "--get",
                "core.repositoryformatversion",
            ],
            None,
        )
        .await
        .unwrap_or_default();
    if format_version.trim() == "0" || format_version.trim().is_empty() {
        runner
            .run(
                "git",
                &[
                    "-C",
                    &repo.to_string_lossy(),
                    "config",
                    "core.repositoryformatversion",
                    "1",
                ],
                None,
                &jackin_core::RunOptions::default(),
            )
            .await?;
    }
    runner
        .run(
            "git",
            &[
                "-C",
                &repo.to_string_lossy(),
                "config",
                "extensions.worktreeConfig",
                "true",
            ],
            None,
            &jackin_core::RunOptions::default(),
        )
        .await?;
    Ok(true)
}

fn find_local_branch_tip(repo: &str, branch: &str) -> Option<String> {
    let git_dir = Path::new(repo).join(".git");
    let mut loose = git_dir.join("refs").join("heads");
    for segment in branch.split('/') {
        loose = loose.join(segment);
    }
    match std::fs::read_to_string(&loose) {
        Ok(contents) => {
            let sha = contents.trim();

            if !sha.is_empty() && !sha.starts_with("ref:") {
                return Some(sha.to_owned());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            let _warning = jackin_telemetry::record_recovered_degradation();
        }
    }
    let packed = git_dir.join("packed-refs");
    let contents = match std::fs::read_to_string(&packed) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            let _warning = jackin_telemetry::record_recovered_degradation();
            return None;
        }
    };
    let want = format!("refs/heads/{branch}");
    for line in contents.lines() {
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let Some((sha, refname)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if refname.trim() == want {
            return Some(sha.trim().to_owned());
        }
    }
    None
}

use jackin_config::MountConfig;

#[derive(Debug, Clone)]
pub struct PreflightContext {
    pub workspace_label: WorkspaceLabel,
    pub force: bool,
    pub interactive: bool,
}

pub async fn preflight_isolated(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    if mount.readonly {
        return Err(IsolationError::ReadonlyIsolated {
            dst: mount.dst.clone(),
            isolation: mount.isolation,
        }
        .into());
    }

    let normalized =
        jackin_config::normalize_sensitive_mount_sources(std::slice::from_ref(mount), |path| {
            std::fs::canonicalize(path).ok()
        });
    let sensitives = jackin_config::find_sensitive_mounts(&normalized);
    if let Some(s) = sensitives.first() {
        return Err(IsolationError::SensitiveOverlap {
            dst: mount.dst.clone(),
            src: s.src.clone(),
            reason: s.reason.clone(),
            isolation: mount.isolation,
        }
        .into());
    }

    let src = Path::new(&mount.src);

    for marker in &[
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
    ] {
        if src.join(".git").join(marker).exists() {
            return Err(IsolationError::MidOperation {
                dst: mount.dst.clone(),
                src: mount.src.clone(),
                marker: (*marker).to_owned(),
            }
            .into());
        }
    }

    let toplevel = runner
        .capture(
            "git",
            &["-C", &mount.src, "rev-parse", "--show-toplevel"],
            None,
        )
        .await
        .with_context(|| {
            format!(
                "isolated mount `{}`: git rev-parse --show-toplevel",
                mount.dst
            )
        })?;
    let toplevel = toplevel.trim();
    let src_canon =
        std::fs::canonicalize(src).with_context(|| format!("canonicalize {}", mount.src))?;
    let top_canon =
        std::fs::canonicalize(toplevel).with_context(|| format!("canonicalize {toplevel}"))?;
    if src_canon != top_canon {
        return Err(IsolationError::NotRepoRoot {
            dst: mount.dst.clone(),
            src: mount.src.clone(),
            toplevel: toplevel.to_owned(),
        }
        .into());
    }

    check_dirty_tree(mount, ctx, runner).await?;

    Ok(())
}

pub async fn preflight_worktree(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    require_directory_git_metadata(mount)?;
    preflight_isolated(mount, ctx, runner).await
}

fn require_directory_git_metadata(mount: &MountConfig) -> anyhow::Result<()> {
    let git_entry = Path::new(&mount.src).join(".git");
    match std::fs::symlink_metadata(&git_entry) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(metadata) if metadata.file_type().is_file() => anyhow::bail!(
            "isolated worktree mount `{}` uses `{}` as a Git file pointer; worktree isolation requires a repository root with a `.git` directory",
            mount.dst,
            mount.src
        ),
        Ok(_) => anyhow::bail!(
            "isolated worktree mount `{}` has unsupported Git metadata at `{}`; expected a `.git` directory",
            mount.dst,
            mount.src
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("inspect Git metadata for isolated mount `{}`", mount.dst)),
    }
}

async fn check_dirty_tree(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let porcelain = runner
        .capture("git", &["-C", &mount.src, "status", "--porcelain"], None)
        .await
        .with_context(|| format!("isolated mount `{}`: git status --porcelain", mount.dst))?;
    if porcelain.trim().is_empty() {
        return Ok(());
    }
    if ctx.force {
        jackin_diagnostics::emit_compact_line(
            "isolation",
            &format!(
                "[jackin] proceeding with dirty host tree at `{}` (--force)",
                mount.src
            ),
        );
        return Ok(());
    }
    if ctx.interactive {
        return Ok(());
    }
    Err(IsolationError::DirtyTree {
        dst: mount.dst.clone(),
        src: mount.src.clone(),
    }
    .into())
}

pub async fn materialize_workspace(
    resolved: &ResolvedWorkspace,
    container_state_dir: &Path,
    selector_key: &str,
    container_name: &str,
    workspace_name: Option<&jackin_core::WorkspaceName>,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<MaterializedWorkspace> {
    let mut indexed: Vec<(usize, &MountConfig)> = resolved.mounts.iter().enumerate().collect();
    indexed.sort_by_key(|(_, m)| m.dst.trim_end_matches('/').len());

    let mut materialized: Vec<Option<MaterializedMount>> =
        (0..resolved.mounts.len()).map(|_| None).collect();

    for (idx, mount) in indexed {
        let m = match mount.isolation {
            MountIsolation::Shared => MaterializedMount {
                bind_src: mount.src.clone(),
                dst: mount.dst.clone(),
                readonly: mount.readonly,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
            MountIsolation::Worktree => {
                materialize_one(
                    mount,
                    container_state_dir,
                    selector_key,
                    container_name,
                    workspace_name,
                    ctx,
                    runner,
                )
                .await?
            }
            MountIsolation::Clone => {
                materialize_clone(
                    mount,
                    container_state_dir,
                    selector_key,
                    container_name,
                    workspace_name,
                    ctx,
                    runner,
                )
                .await?
            }
        };
        materialized[idx] = Some(m);
    }

    let mounts: Vec<MaterializedMount> = materialized
        .into_iter()
        .collect::<Option<_>>()
        .ok_or(IsolationError::MissingMountSlot)?;
    Ok(MaterializedWorkspace {
        workdir: resolved.workdir.clone(),
        mounts,
        keep_awake_enabled: resolved.keep_awake_enabled,
    })
}

async fn materialize_one(
    mount: &MountConfig,
    container_state_dir: &Path,
    selector_key: &str,
    container_name: &str,
    workspace_name: Option<&jackin_core::WorkspaceName>,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<MaterializedMount> {
    require_directory_git_metadata(mount)?;
    let source_git_dir = std::fs::canonicalize(Path::new(&mount.src).join(".git"))
        .with_context(|| format!("canonicalize common Git directory for {}", mount.src))?;
    let source_git_dir = crate::safe_remove::pin_dir(&source_git_dir)?
        .context("common Git directory disappeared before worktree admission")?;
    anyhow::ensure!(
        source_git_dir.read_file("commondir")?.is_none(),
        "worktree admission requires the source repository's common Git directory"
    );
    let _worktree_registry_lock = source_git_dir
        .lock_worktree_registry()
        .await
        .context("cannot acquire common Git worktree registry lock")?;
    let journal_name = worktree_cleanup_journal_name(&mount.dst);
    anyhow::ensure!(
        read_worktree_cleanup(container_state_dir, &journal_name)?.is_none(),
        "worktree cleanup for mount `{}` is incomplete; purge must resume it before materialization",
        mount.dst
    );
    let worktree_path = worktree_path_for(container_state_dir, &mount.dst, container_name);

    if let Some(record) = read_record(container_state_dir, &mount.dst)? {
        anyhow::ensure!(
            record.workspace_name.as_ref() == workspace_name,
            "isolated mount `{}` belongs to a different saved workspace; preserve the state and rebuild this instance from its recorded workspace",
            mount.dst,
        );
        if record.original_src != mount.src {
            return Err(IsolationError::SourceDrift {
                container: container_name.into(),
                mount: mount.dst.clone(),
                recorded: record.original_src.clone(),
                configured: mount.src.clone(),
                isolation: record.isolation,
                worktree: record.worktree_path.clone(),
            }
            .into());
        }
        if record.isolation != MountIsolation::Worktree {
            return Err(IsolationError::ModeDrift {
                container: container_name.into(),
                mount: mount.dst.clone(),
                recorded: record.isolation,
                configured: mount.isolation,
                worktree: record.worktree_path.clone(),
            }
            .into());
        }

        if worktree_path.join(".git").exists() {
            let aux =
                write_git_overrides(container_state_dir, &mount.dst, container_name, &mount.src)?;
            return Ok(MaterializedMount {
                bind_src: worktree_path.to_string_lossy().into(),
                dst: mount.dst.clone(),
                readonly: mount.readonly,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(aux),
            });
        }
    }

    preflight_worktree(mount, ctx, runner).await?;

    let _ = ensure_worktree_config_enabled(Path::new(&mount.src), runner).await?;

    let host_head = runner
        .capture("git", &["-C", &mount.src, "rev-parse", "HEAD"], None)
        .await?
        .trim()
        .to_owned();

    let scratch_branch = branch_name(container_name, None);
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent dir for worktree at {}", parent.display()))?;
    }

    let base_commit = if find_local_branch_tip(&mount.src, &scratch_branch).is_some() {
        runner
            .run(
                "git",
                &["-C", &mount.src, "worktree", "prune"],
                None,
                &jackin_core::RunOptions::default(),
            )
            .await?;
        runner
            .run(
                "git",
                &[
                    "-C",
                    &mount.src,
                    "worktree",
                    "add",
                    &worktree_path.to_string_lossy(),
                    &scratch_branch,
                ],
                None,
                &jackin_core::RunOptions::default(),
            )
            .await
            .with_context(|| {
                format!(
                    "isolated mount `{}`: adopt of existing scratch branch `{}` failed; \
                     if the branch is checked out in another worktree, \
                     `git -C {} worktree list --porcelain` will show where",
                    mount.dst, scratch_branch, mount.src,
                )
            })?;
        host_head.clone()
    } else {
        runner
            .run(
                "git",
                &[
                    "-C",
                    &mount.src,
                    "worktree",
                    "add",
                    "-b",
                    &scratch_branch,
                    &worktree_path.to_string_lossy(),
                    &host_head,
                ],
                None,
                &jackin_core::RunOptions::default(),
            )
            .await?;
        host_head
    };

    upsert_record(
        container_state_dir,
        IsolationRecord {
            workspace_name: workspace_name.cloned(),
            mount_dst: mount.dst.clone(),
            original_src: mount.src.clone(),
            isolation: MountIsolation::Worktree,
            worktree_path: worktree_path.to_string_lossy().into(),
            scratch_branch,
            base_commit,
            selector_key: selector_key.into(),
            container_name: container_name.into(),
            cleanup_status: CleanupStatus::Active,
        },
    )?;

    let aux = write_git_overrides(container_state_dir, &mount.dst, container_name, &mount.src)?;
    Ok(MaterializedMount {
        bind_src: worktree_path.to_string_lossy().into(),
        dst: mount.dst.clone(),
        readonly: mount.readonly,
        isolation: MountIsolation::Worktree,
        worktree_aux: Some(aux),
    })
}

async fn materialize_clone(
    mount: &MountConfig,
    container_state_dir: &Path,
    selector_key: &str,
    container_name: &str,
    workspace_name: Option<&jackin_core::WorkspaceName>,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<MaterializedMount> {
    let clone_path = clone_path_for(container_state_dir, &mount.dst, container_name);
    if let Some(record) = read_record(container_state_dir, &mount.dst)? {
        anyhow::ensure!(
            record.workspace_name.as_ref() == workspace_name,
            "isolated mount `{}` belongs to a different saved workspace; preserve the state and rebuild this instance from its recorded workspace",
            mount.dst,
        );
        if record.original_src != mount.src {
            return Err(IsolationError::SourceDrift {
                container: container_name.into(),
                mount: mount.dst.clone(),
                recorded: record.original_src.clone(),
                configured: mount.src.clone(),
                isolation: record.isolation,
                worktree: record.worktree_path.clone(),
            }
            .into());
        }
        if record.isolation != MountIsolation::Clone {
            return Err(IsolationError::ModeDrift {
                container: container_name.into(),
                mount: mount.dst.clone(),
                recorded: record.isolation,
                configured: mount.isolation,
                worktree: record.worktree_path.clone(),
            }
            .into());
        }
        if clone_path.join(".git").exists() {
            return Ok(MaterializedMount {
                bind_src: clone_path.to_string_lossy().into(),
                dst: mount.dst.clone(),
                readonly: mount.readonly,
                isolation: MountIsolation::Clone,
                worktree_aux: None,
            });
        }
    }

    preflight_isolated(mount, ctx, runner).await?;

    let host_head = runner
        .capture("git", &["-C", &mount.src, "rev-parse", "HEAD"], None)
        .await?
        .trim()
        .to_owned();

    if let Some(parent) = clone_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent dir for clone at {}", parent.display()))?;
    }

    runner
        .run(
            "git",
            &[
                "clone",
                "--local",
                &mount.src,
                &clone_path.to_string_lossy(),
            ],
            None,
            &jackin_core::RunOptions::default(),
        )
        .await?;

    let host_origin = match runner
        .capture(
            "git",
            &["-C", &mount.src, "remote", "get-url", "origin"],
            None,
        )
        .await
    {
        Ok(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(strip_userinfo(normalize_github_url(trimmed)))
            }
        }
        Err(err) => {
            let chain = format!("{err:#}");
            if chain.contains("No such remote") || chain.contains("no such remote") {
                None
            } else {
                return Err(err).with_context(|| {
                    format!(
                        "isolated mount `{}`: failed to read host repo `{}` origin URL — \
                         refusing to launch with a loopback origin that would silently \
                         misroute pushes. If the host repo legitimately has no origin, \
                         add one (`git remote add origin <url>`) or switch this mount to \
                         `isolation = \"shared\"` / `\"worktree\"`",
                        mount.dst, mount.src,
                    )
                });
            }
        }
    };

    if let Some(url) = host_origin {
        runner
            .run(
                "git",
                &[
                    "-C",
                    &clone_path.to_string_lossy(),
                    "remote",
                    "set-url",
                    "origin",
                    &url,
                ],
                None,
                &jackin_core::RunOptions::default(),
            )
            .await?;
    }

    upsert_record(
        container_state_dir,
        IsolationRecord {
            workspace_name: workspace_name.cloned(),
            mount_dst: mount.dst.clone(),
            original_src: mount.src.clone(),
            isolation: MountIsolation::Clone,
            worktree_path: clone_path.to_string_lossy().into(),
            scratch_branch: String::new(),
            base_commit: host_head,
            selector_key: selector_key.into(),
            container_name: container_name.into(),
            cleanup_status: CleanupStatus::Active,
        },
    )?;

    Ok(MaterializedMount {
        bind_src: clone_path.to_string_lossy().into(),
        dst: mount.dst.clone(),
        readonly: mount.readonly,
        isolation: MountIsolation::Clone,
        worktree_aux: None,
    })
}

pub fn mount_order_for_docker(mat: &MaterializedWorkspace) -> Vec<&MaterializedMount> {
    let mut ordered: Vec<&MaterializedMount> = mat.mounts.iter().collect();
    ordered.sort_by_key(|m| m.dst.trim_end_matches('/').len());
    ordered
}

#[cfg(test)]
mod tests;
