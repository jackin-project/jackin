use crate::debug_log;
use crate::docker::CommandRunner;
use crate::isolation::MountIsolation;
use crate::isolation::branch::branch_name;
use crate::isolation::state::{CleanupStatus, IsolationRecord, read_record, upsert_record};
use crate::workspace::ResolvedWorkspace;
use anyhow::Context;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedWorkspace {
    pub workdir: String,
    pub mounts: Vec<MaterializedMount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedMount {
    pub bind_src: String,
    pub dst: String,
    pub readonly: bool,
    pub isolation: MountIsolation,
    /// Auxiliary docker bind mounts required so git can operate on this
    /// worktree from inside the container. `None` for `Shared` mounts.
    /// See the per-mount-isolation roadmap entry "Container-side mount
    /// layout" and "Design Decision: Worktree Materialization Layout"
    /// for the rationale and topology.
    pub worktree_aux: Option<WorktreeAuxMounts>,
}

/// Three extra bind mounts the container needs so a worktree's gitdir
/// relationship resolves consistently inside the container.
///
/// Single top-level container path: everything jackin contributes lives
/// under `/jackin/host/<dst-stripped>/.git/`. The host repo's full
/// `.git/` is bind-mounted there (rw); the admin dir for this
/// worktree is at `worktrees/<container>/` natively (part of the same
/// mount). The two override files are file-level overlays inside that
/// directory mount: one shadowing the worktree's `.git` text file (so
/// the agent's gitdir resolves to a container path), one shadowing the
/// admin's `gitdir` back-pointer (so git's integrity check passes
/// where `<dst>/.git` differs from the host worktree path). All
/// sources are either jackin-owned (override files under the container
/// state dir) or host-owned via bind mount; host worktree files are
/// never modified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeAuxMounts {
    /// Host source: `<host_repo>/.git`. Container target:
    /// `/jackin/host/<dst-stripped>/.git`. Read-write — git writes
    /// refs, objects, HEAD/index/logs all under here on every
    /// commit/branch/fetch. The destination intentionally mirrors the
    /// host topology so that `docker inspect` shows symmetric
    /// Source/Destination paths both ending in `.git`. Includes the
    /// per-worktree admin dir at `worktrees/<container>/` natively —
    /// no separate admin mount is needed; the on-disk `commondir`
    /// (`../..`) resolves correctly inside this container path.
    pub host_git_dir: String,
    pub host_git_target: String,
    /// Host source: jackin-owned override `.git` file containing
    /// `gitdir: /jackin/host/<dst-stripped>/.git/worktrees/<container>`.
    /// Container target: `<dst>/.git`. Redirects the worktree's gitdir
    /// to the admin entry inside the host `.git/` mount.
    pub git_file_override: String,
    pub git_file_target: String,
    /// Host source: jackin-owned override file containing `<dst>/.git`.
    /// Container target:
    /// `/jackin/host/<dst-stripped>/.git/worktrees/<container>/gitdir`.
    /// Overrides git's admin-dir back-pointer so its verification check
    /// (back-pointer must match the worktree's `.git` location) passes
    /// inside the container, where `<dst>` differs from the host
    /// worktree path. This is a file-level overlay on top of a
    /// directory-level mount (the host `.git/` mount) — Docker handles
    /// this natively by shadowing only the single file.
    pub gitdir_back_override: String,
    pub gitdir_back_target: String,
}

/// Compute the host-side worktree path for an isolated mount.
///
/// The path's *basename* matters: it's what `git worktree add` uses
/// as the admin entry name in `<host_repo>/.git/worktrees/<n>/`. We
/// use the container name as the basename so admin entries are
/// globally unique per (`host_repo`, container) — `git worktree list`
/// on the host shows which container owns each worktree at a glance.
/// No auto-suffix needed; no read-back required.
/// `validate_isolation_layout` already rejects two isolated mounts on
/// the same host repo within one workspace, so the basename can never
/// collide with itself; cross-container collisions are avoided by
/// container-name uniqueness.
///
/// On-disk layout (groups all git-related artifacts under
/// `<container_state>/git/`, with `worktree/repo/` marking the subtree
/// as git-managed and `overrides/` holding jackin's bind-mount sources):
///
/// ```text
/// <container_state>/git/
/// ├── worktree/repo/<dst-stripped>/<container>/   ← THIS path (the materialized git worktree)
/// └── overrides/<dst-stripped>/                    ← jackin-owned override files (see write_git_overrides)
///     ├── .git
///     └── gitdir
/// ```
pub fn worktree_path_for(container_state_dir: &Path, dst: &str, container_name: &str) -> PathBuf {
    let rel = dst.trim_matches('/');
    container_state_dir
        .join("git")
        .join("worktree")
        .join("repo")
        .join(rel)
        .join(container_name)
}

/// Container-side path where the host repo's `.git/` is bind-mounted.
/// Mirrors the host topology under `/jackin/host/` so:
///
///   docker inspect Source       = `<host_repo>/.git`
///   docker inspect Destination  = `/jackin/host/<dst-stripped>/.git`
///
/// reads symmetrically — both ends terminate in `.git`. Per-mount
/// disambiguation comes from `<dst-stripped>` (= `dst.trim_matches('/')`,
/// slashes preserved as directory separators), matching the scheme
/// `worktree_path_for` already uses for the worktree itself.
fn container_host_git_path(mount_dst: &str) -> String {
    let rel = mount_dst.trim_matches('/');
    format!("/jackin/host/{rel}/.git")
}

/// Write the two jackin-owned override files alongside the materialized
/// worktree. Idempotent: rewrites both files on every call so reused
/// worktrees pick up any topology changes (rare, but cheap).
///
/// Storage layout: every git-related artifact for one mount lives
/// under `<container_state>/git/`, with `worktree/repo/` marking the
/// subtree as git-managed and `overrides/` holding jackin's
/// bind-mount sources. Override-file names match their docker mount
/// destinations:
///
/// ```text
/// <container_state>/git/
/// ├── worktree/repo/<dst-stripped>/<container>/  (materialized by `git worktree add`; see worktree_path_for)
/// └── overrides/<dst-stripped>/
///     ├── .git    → mounted at <dst>/.git (`:ro`)
///     └── gitdir  → mounted at /jackin/host/<dst-stripped>/.git/worktrees/<container>/gitdir (`:ro`)
/// ```
///
/// `.git` redirects gitdir to the admin entry inside the host `.git/`
/// mount (`/jackin/host/<dst-stripped>/.git/worktrees/<container>`).
/// `gitdir` is the back-pointer matching the worktree's `.git`
/// location inside the container (`<dst>/.git`).
///
/// No `commondir` override is needed: with the admin dir living
/// natively inside the host `.git/` mount at `worktrees/<container>/`,
/// the on-disk default `commondir = ../..` resolves correctly inside
/// the container (to the shared `.git/`).
///
/// Returns the [`WorktreeAuxMounts`] needed to wire up the three
/// auxiliary bind mounts at docker-run time.
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

    // Override 1 (`.git`): replacement worktree pointer file. Mounted
    // at `<dst>/.git` inside the container. Redirects gitdir to the
    // admin entry inside the host `.git/` mount at
    // `worktrees/<container>/`. Admin name = container name
    // (deterministic — validation rejects same-host-repo siblings,
    // and container names are globally unique, so no auto-suffix or
    // read-back is required).
    let git_file_override_path = mount_overrides_dir.join(".git");
    let git_file_content = format!("gitdir: {host_git_target}/worktrees/{container_name}\n");
    std::fs::write(&git_file_override_path, &git_file_content)
        .with_context(|| format!("write .git override {}", git_file_override_path.display()))?;

    // Override 2 (`gitdir`): replacement back-pointer mounted at
    // `<host_git_target>/worktrees/<container>/gitdir`. Tells git
    // "the worktree's `.git` file is at `<dst>/.git`" so its
    // verification check passes (the host's absolute path stored in
    // the on-disk back-pointer would NOT match `<dst>` inside the
    // container, hence the override).
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

/// Enable `extensions.worktreeConfig` on a host repo if not already set.
/// Returns Ok(true) when newly enabled (caller may print a notice),
/// Ok(false) when already enabled.
pub fn ensure_worktree_config_enabled(
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
        .unwrap_or_default();
    if current.trim() == "true" {
        debug_log!(
            "isolation",
            "extensions.worktreeConfig already enabled at {}",
            repo.display()
        );
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
        .unwrap_or_default();
    if format_version.trim() == "0" || format_version.trim().is_empty() {
        debug_log!(
            "isolation",
            "bumping core.repositoryformatversion 0 -> 1 at {} (required for extensions.worktreeConfig)",
            repo.display()
        );
        runner.run(
            "git",
            &[
                "-C",
                &repo.to_string_lossy(),
                "config",
                "core.repositoryformatversion",
                "1",
            ],
            None,
            &crate::docker::RunOptions::default(),
        )?;
    }
    debug_log!(
        "isolation",
        "enabling extensions.worktreeConfig at {} (per-worktree config from now on)",
        repo.display()
    );
    runner.run(
        "git",
        &[
            "-C",
            &repo.to_string_lossy(),
            "config",
            "extensions.worktreeConfig",
            "true",
        ],
        None,
        &crate::docker::RunOptions::default(),
    )?;
    Ok(true)
}

use crate::workspace::MountConfig;

#[derive(Debug, Clone)]
pub struct PreflightContext {
    pub workspace_name: String,
    pub force: bool,
    pub interactive: bool,
}

/// Validation that must pass before `git worktree add`. Layout validation
/// (parent/child rejection) happens earlier at config-validation time;
/// this is per-mount.
pub fn preflight_worktree(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    // readonly is incompatible with worktree mode.
    anyhow::ensure!(
        !mount.readonly,
        "isolated mount `{}` cannot be readonly (isolation = worktree)",
        mount.dst
    );

    // Sensitive mount overlap.
    let sensitives = crate::workspace::find_sensitive_mounts(std::slice::from_ref(mount));
    if let Some(s) = sensitives.first() {
        anyhow::bail!(
            "isolated mount `{}` overlaps sensitive path `{}` ({}) (isolation = worktree)",
            mount.dst,
            s.src,
            s.reason
        );
    }

    let src = std::path::Path::new(&mount.src);

    // Mid-rebase / merge / cherry-pick guard.
    for marker in &[
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
    ] {
        if src.join(".git").join(marker).exists() {
            anyhow::bail!(
                "isolated mount `{}`: host repo `{}` is mid-{}; resolve before launching",
                mount.dst,
                mount.src,
                marker
            );
        }
    }

    // src must be a git repo *root* — toplevel must equal src.
    let toplevel = runner
        .capture(
            "git",
            &["-C", &mount.src, "rev-parse", "--show-toplevel"],
            None,
        )
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
    anyhow::ensure!(
        src_canon == top_canon,
        "isolated mount `{}`: src `{}` is inside repo `{}` but not its root",
        mount.dst,
        mount.src,
        toplevel
    );

    // Dirty tree check (separate test in 4.5).
    check_dirty_tree(mount, ctx, runner)?;

    Ok(())
}

fn check_dirty_tree(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let porcelain = runner
        .capture("git", &["-C", &mount.src, "status", "--porcelain"], None)
        .with_context(|| format!("isolated mount `{}`: git status --porcelain", mount.dst))?;
    if porcelain.trim().is_empty() {
        return Ok(());
    }
    if ctx.force {
        eprintln!(
            "[jackin] proceeding with dirty host tree at `{}` (--force)",
            mount.src
        );
        return Ok(());
    }
    if ctx.interactive {
        return Ok(());
    }
    anyhow::bail!(
        "isolated mount `{}`: host tree at `{}` is dirty (staged/unstaged/untracked); \
         pass --force to acknowledge, or commit/stash before launching",
        mount.dst,
        mount.src
    );
}

/// Top-level materialization.
///
/// Iterates the resolved workspace mounts, passes through `Shared` mounts,
/// and per-mount-materializes `Worktree` mounts. Returns the
/// `MaterializedWorkspace` ready for Docker launch.
pub fn materialize_workspace(
    resolved: &ResolvedWorkspace,
    container_state_dir: &Path,
    selector_key: &str,
    container_name: &str,
    workspace_name: &str,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<MaterializedWorkspace> {
    let isolated_count = resolved
        .mounts
        .iter()
        .filter(|m| matches!(m.isolation, MountIsolation::Worktree))
        .count();
    debug_log!(
        "isolation",
        "materialize_workspace: workspace={workspace_name} container={container_name} selector={selector_key} mounts={total} isolated={isolated_count} state_dir={state_dir} force={force} interactive={interactive}",
        total = resolved.mounts.len(),
        state_dir = container_state_dir.display(),
        force = ctx.force,
        interactive = ctx.interactive,
    );

    // Sort by dst length ascending so parents materialize before children
    // (depth ordering for the bind-mount stack).
    let mut indexed: Vec<(usize, &MountConfig)> = resolved.mounts.iter().enumerate().collect();
    indexed.sort_by_key(|(_, m)| m.dst.trim_end_matches('/').len());

    let mut materialized: Vec<Option<MaterializedMount>> =
        (0..resolved.mounts.len()).map(|_| None).collect();

    for (idx, mount) in indexed {
        let m = match mount.isolation {
            MountIsolation::Shared => {
                debug_log!(
                    "isolation",
                    "mount {dst}: shared (passthrough bind from {src})",
                    dst = mount.dst,
                    src = mount.src,
                );
                MaterializedMount {
                    bind_src: mount.src.clone(),
                    dst: mount.dst.clone(),
                    readonly: mount.readonly,
                    isolation: MountIsolation::Shared,
                    worktree_aux: None,
                }
            }
            MountIsolation::Worktree => materialize_one(
                mount,
                container_state_dir,
                selector_key,
                container_name,
                workspace_name,
                ctx,
                runner,
            )?,
            MountIsolation::Clone => {
                anyhow::bail!(
                    "isolated mount `{}`: clone mode is reserved but not implemented yet",
                    mount.dst
                )
            }
        };
        materialized[idx] = Some(m);
    }

    // Re-emit in original order — Docker mount-flag order is settled later.
    let mounts: Vec<MaterializedMount> = materialized
        .into_iter()
        .map(|m| m.expect("every mount index populated by the loop above"))
        .collect();
    Ok(MaterializedWorkspace {
        workdir: resolved.workdir.clone(),
        mounts,
    })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn materialize_one(
    mount: &MountConfig,
    container_state_dir: &Path,
    selector_key: &str,
    container_name: &str,
    workspace_name: &str,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<MaterializedMount> {
    let worktree_path = worktree_path_for(container_state_dir, &mount.dst, container_name);
    debug_log!(
        "isolation",
        "mount {dst}: worktree (src={src} → worktree_path={wt})",
        dst = mount.dst,
        src = mount.src,
        wt = worktree_path.display(),
    );

    // Drift guard: if a record exists, src must match.
    if let Some(record) = read_record(container_state_dir, &mount.dst)? {
        if record.original_src != mount.src {
            debug_log!(
                "isolation",
                "mount {dst}: source drift detected (recorded={recorded} configured={configured})",
                dst = mount.dst,
                recorded = record.original_src,
                configured = mount.src,
            );
            anyhow::bail!(
                "source drift on container `{}`, mount `{}`: recorded src `{}` differs from configured src `{}`; preserved worktree at `{}`. Restore the previous src, run `jackin cd {} {}` to inspect, or `jackin purge {}` to discard.",
                container_name,
                mount.dst,
                record.original_src,
                mount.src,
                record.worktree_path,
                container_name,
                mount.dst,
                container_name,
            );
        }
        // Reuse if worktree path looks alive (.git file or dir under it).
        if worktree_path.join(".git").exists() {
            debug_log!(
                "isolation",
                "mount {dst}: reusing existing worktree (record matches and .git present)",
                dst = mount.dst,
            );
            // Re-write override files on every load — idempotent and
            // cheap, ensures any topology refresh (e.g., container
            // rename hypothetically) lands without manual cleanup.
            let aux =
                write_git_overrides(container_state_dir, &mount.dst, container_name, &mount.src)?;
            debug_log!(
                "isolation",
                "mount {dst}: refreshed git overrides (host_git_target={target})",
                dst = mount.dst,
                target = aux.host_git_target,
            );
            return Ok(MaterializedMount {
                bind_src: worktree_path.to_string_lossy().into(),
                dst: mount.dst.clone(),
                readonly: mount.readonly,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(aux),
            });
        }
        debug_log!(
            "isolation",
            "mount {dst}: record present but worktree directory missing — recreating",
            dst = mount.dst,
        );
    }

    // Pre-flight, then enable worktree-config, then create the worktree.
    debug_log!(
        "isolation",
        "mount {dst}: running preflight checks on host repo {src}",
        dst = mount.dst,
        src = mount.src,
    );
    preflight_worktree(mount, ctx, runner)?;

    let _ = ensure_worktree_config_enabled(std::path::Path::new(&mount.src), runner)?;

    let base_commit = runner
        .capture("git", &["-C", &mount.src, "rev-parse", "HEAD"], None)?
        .trim()
        .to_string();
    debug_log!(
        "isolation",
        "mount {dst}: base commit {commit} from host HEAD",
        dst = mount.dst,
        commit = base_commit,
    );

    // No per-mount branch suffix in V1: workspace validation rejects
    // two isolated mounts on the same host repo (see
    // `validate_isolation_layout`), so each container has at most one
    // isolated mount per host repo and the scratch branch is uniquely
    // named by the selector alone.
    // Branch name = `jackin/scratch/<container>` (Model B). Container
    // name is the disambiguator because it's globally unique by jackin
    // construction; selector alone wouldn't disambiguate parallel
    // containers of the same agent class (which would collide on the
    // shared host repo's `<host>/.git/refs/heads/` namespace).
    let scratch_branch = branch_name(container_name, None);
    debug_log!(
        "isolation",
        "mount {dst}: scratch branch {branch} (selector={selector})",
        dst = mount.dst,
        branch = scratch_branch,
        selector = selector_key,
    );

    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent dir for worktree at {}", parent.display()))?;
    }

    debug_log!(
        "isolation",
        "mount {dst}: git -C {src} worktree add -b {branch} {wt} {base}",
        dst = mount.dst,
        src = mount.src,
        branch = scratch_branch,
        wt = worktree_path.display(),
        base = base_commit,
    );
    runner.run(
        "git",
        &[
            "-C",
            &mount.src,
            "worktree",
            "add",
            "-b",
            &scratch_branch,
            &worktree_path.to_string_lossy(),
            &base_commit,
        ],
        None,
        &crate::docker::RunOptions::default(),
    )?;

    upsert_record(
        container_state_dir,
        IsolationRecord {
            workspace: workspace_name.into(),
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
    debug_log!(
        "isolation",
        "mount {dst}: wrote git overrides (host_git_target={t}, git_file_target={gft}, gitdir_back_target={gbt})",
        dst = mount.dst,
        t = aux.host_git_target,
        gft = aux.git_file_target,
        gbt = aux.gitdir_back_target,
    );

    Ok(MaterializedMount {
        bind_src: worktree_path.to_string_lossy().into(),
        dst: mount.dst.clone(),
        readonly: mount.readonly,
        isolation: MountIsolation::Worktree,
        worktree_aux: Some(aux),
    })
}

/// Order mounts so parents appear before children. Docker overlays later
/// mounts on earlier ones, so this lets a shared cache child mount land
/// inside an isolated worktree parent.
pub fn mount_order_for_docker(mat: &MaterializedWorkspace) -> Vec<&MaterializedMount> {
    let mut ordered: Vec<&MaterializedMount> = mat.mounts.iter().collect();
    ordered.sort_by_key(|m| m.dst.trim_end_matches('/').len());
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn materialized_mount_holds_isolation() {
        let m = MaterializedMount {
            bind_src: "/tmp/a".into(),
            dst: "/workspace/a".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
            worktree_aux: None,
        };
        assert_eq!(m.isolation, MountIsolation::Worktree);
    }

    #[test]
    fn worktree_path_uses_container_name_as_basename() {
        // Container name as the worktree's basename gives globally
        // unique admin entry names in `<host_repo>/.git/worktrees/`.
        // The `worktree/repo/` segments mark git's territory inside
        // the per-container state dir.
        let base = PathBuf::from("/data/jackin-x");
        assert_eq!(
            worktree_path_for(&base, "/workspace/jackin", "jackin-the-architect"),
            PathBuf::from("/data/jackin-x/git/worktree/repo/workspace/jackin/jackin-the-architect"),
        );
    }

    #[test]
    fn worktree_path_strips_trailing_slash_in_dst() {
        let base = PathBuf::from("/data/jackin-x");
        assert_eq!(
            worktree_path_for(&base, "/workspace/jackin/", "jackin-x"),
            PathBuf::from("/data/jackin-x/git/worktree/repo/workspace/jackin/jackin-x"),
        );
    }

    #[test]
    fn container_host_git_path_mirrors_dst_under_jackin_host() {
        assert_eq!(
            container_host_git_path("/Users/donbeave/Projects/jackin-project/jackin"),
            "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git",
            "host .git destination mirrors host topology under /jackin/host/, ends in .git",
        );
        assert_eq!(
            container_host_git_path("/workspace/jackin/"),
            "/jackin/host/workspace/jackin/.git",
            "trailing slash on dst is stripped",
        );
    }

    #[test]
    fn host_git_paths_disambiguate_per_mount_in_one_container() {
        // Two isolated mounts on different host repos in the same
        // container must land at distinct container paths so multi-mount
        // workspaces don't collide. With the admin entry living
        // natively inside the host `.git/` mount, this single check is
        // enough — admin disambiguation is inherited from the host
        // mount disambiguation.
        assert_ne!(
            container_host_git_path("/workspace/proj-a"),
            container_host_git_path("/workspace/proj-b"),
        );
    }

    #[test]
    fn write_git_overrides_writes_two_files_with_correct_content() {
        let cdir = tempfile::TempDir::new().unwrap();

        let aux = write_git_overrides(
            cdir.path(),
            "/workspace/jackin",
            "jackin-the-architect",
            "/host/jackin",
        )
        .unwrap();

        // Auxiliary mount metadata reflects the design doc topology:
        // /jackin/host/<dst-tree>/.git for the host repo's .git/ mount,
        // with both override files layered on top of that mount.
        assert_eq!(aux.host_git_dir, "/host/jackin/.git");
        assert_eq!(aux.host_git_target, "/jackin/host/workspace/jackin/.git");
        assert_eq!(aux.git_file_target, "/workspace/jackin/.git");
        // gitdir back-pointer override target lives natively at
        // worktrees/<container>/gitdir inside the host .git/ mount.
        assert_eq!(
            aux.gitdir_back_target,
            "/jackin/host/workspace/jackin/.git/worktrees/jackin-the-architect/gitdir",
        );

        // Override file contents.
        let git_file = std::fs::read_to_string(&aux.git_file_override).unwrap();
        assert_eq!(
            git_file, "gitdir: /jackin/host/workspace/jackin/.git/worktrees/jackin-the-architect\n",
            "git-file redirects gitdir to the admin entry inside the host .git/ mount",
        );
        let gitdir_back = std::fs::read_to_string(&aux.gitdir_back_override).unwrap();
        assert_eq!(gitdir_back, "/workspace/jackin/.git\n");

        // Override files live under git/overrides/<dst-tree>/ on host,
        // with filenames matching their docker mount destinations
        // (.git, gitdir). No commondir override — git's on-disk default
        // (`commondir = ../..`) resolves correctly because the admin
        // entry is in-place inside the host .git/ mount.
        assert!(
            aux.git_file_override
                .ends_with("/git/overrides/workspace/jackin/.git"),
            "got {}",
            aux.git_file_override
        );
        assert!(
            aux.gitdir_back_override
                .ends_with("/git/overrides/workspace/jackin/gitdir"),
            "got {}",
            aux.gitdir_back_override
        );
    }

    #[test]
    fn write_git_overrides_is_idempotent() {
        let cdir = tempfile::TempDir::new().unwrap();

        let first = write_git_overrides(
            cdir.path(),
            "/workspace/jackin",
            "jackin-the-architect",
            "/host/jackin",
        )
        .unwrap();
        let second = write_git_overrides(
            cdir.path(),
            "/workspace/jackin",
            "jackin-the-architect",
            "/host/jackin",
        )
        .unwrap();
        // Same paths, same content — re-running on a reused worktree is safe.
        assert_eq!(first, second);
    }

    use crate::runtime::test_support::FakeRunner;
    use std::collections::VecDeque;

    fn fake_with_outputs(outputs: &[&str]) -> FakeRunner {
        FakeRunner {
            capture_queue: VecDeque::from(
                outputs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ),
            ..Default::default()
        }
    }

    #[test]
    fn worktree_config_skips_when_already_enabled() {
        let mut runner = fake_with_outputs(&["true\n"]);
        let newly = ensure_worktree_config_enabled(Path::new("/repo"), &mut runner).unwrap();
        assert!(!newly);
        assert_eq!(runner.run_recorded.len(), 0);
    }

    #[test]
    fn worktree_config_enables_and_bumps_format_version_from_zero() {
        let mut runner = fake_with_outputs(&["", "0"]);
        let newly = ensure_worktree_config_enabled(Path::new("/repo"), &mut runner).unwrap();
        assert!(newly);
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("core.repositoryformatversion 1"))
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("extensions.worktreeConfig true"))
        );
    }

    #[test]
    fn worktree_config_skips_format_bump_when_already_one() {
        let mut runner = fake_with_outputs(&["", "1"]);
        ensure_worktree_config_enabled(Path::new("/repo"), &mut runner).unwrap();
        assert!(
            !runner
                .run_recorded
                .iter()
                .any(|c| c.contains("core.repositoryformatversion"))
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("extensions.worktreeConfig true"))
        );
    }

    use crate::workspace::MountConfig;

    fn ctx() -> PreflightContext {
        PreflightContext {
            workspace_name: "jackin".into(),
            force: false,
            interactive: false,
        }
    }

    fn worktree_mount(dst: &str, src: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }
    }

    #[test]
    fn preflight_rejects_readonly() {
        let mut m = worktree_mount("/workspace/x", "/tmp/x");
        m.readonly = true;
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("cannot be readonly"));
    }

    #[test]
    fn preflight_rejects_sensitive_mount() {
        let home = directories::BaseDirs::new()
            .unwrap()
            .home_dir()
            .to_path_buf();
        let m = worktree_mount("/workspace/ssh", &home.join(".ssh").to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("sensitive"));
    }

    #[test]
    fn preflight_rejects_mid_rebase() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git/rebase-merge")).unwrap();
        let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("mid-rebase-merge"));
    }

    #[test]
    fn preflight_rejects_mid_merge() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/MERGE_HEAD"), "x").unwrap();
        let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("mid-MERGE_HEAD"));
    }

    #[test]
    fn preflight_rejects_mid_cherry_pick() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/CHERRY_PICK_HEAD"), "x").unwrap();
        let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("mid-CHERRY_PICK_HEAD"));
    }

    #[test]
    fn preflight_rejects_subdir_of_repo() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let m = worktree_mount("/workspace/x", &sub.to_string_lossy());
        let mut runner = fake_with_outputs(&[&dir.path().to_string_lossy()]);
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("not its root"));
    }

    fn dirty_porcelain() -> &'static str {
        " M src/foo.rs\n?? new.rs\n"
    }

    fn ignored_only_porcelain() -> &'static str {
        ""
    }

    fn make_repo_root() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        dir
    }

    fn fake_with_repo_and_status(repo: &std::path::Path, status: &str) -> FakeRunner {
        // Capture queue order: rev-parse --show-toplevel, status --porcelain
        fake_with_outputs(&[&repo.to_string_lossy(), status])
    }

    #[test]
    fn dirty_tree_rejected_non_interactive_no_force() {
        let repo = make_repo_root();
        let m = worktree_mount("/workspace/x", &repo.path().to_string_lossy());
        let mut runner = fake_with_repo_and_status(repo.path(), dirty_porcelain());
        let mut c = ctx();
        c.force = false;
        c.interactive = false;
        let err = preflight_worktree(&m, &c, &mut runner).unwrap_err();
        assert!(err.to_string().contains("dirty"));
        assert!(err.to_string().contains("--force"));
    }

    #[test]
    fn dirty_tree_passes_with_force_non_interactive() {
        let repo = make_repo_root();
        let m = worktree_mount("/workspace/x", &repo.path().to_string_lossy());
        let mut runner = fake_with_repo_and_status(repo.path(), dirty_porcelain());
        let mut c = ctx();
        c.force = true;
        preflight_worktree(&m, &c, &mut runner).unwrap();
    }

    #[test]
    fn clean_tree_passes() {
        let repo = make_repo_root();
        let m = worktree_mount("/workspace/x", &repo.path().to_string_lossy());
        let mut runner = fake_with_repo_and_status(repo.path(), ignored_only_porcelain());
        preflight_worktree(&m, &ctx(), &mut runner).unwrap();
    }

    use crate::isolation::state::{CleanupStatus, read_records};
    use crate::workspace::ResolvedWorkspace;

    fn resolved_with_one_isolated(repo: &std::path::Path, dst: &str) -> ResolvedWorkspace {
        ResolvedWorkspace {
            label: "jackin".into(),
            workdir: dst.into(),
            mounts: vec![MountConfig {
                src: repo.to_string_lossy().into(),
                dst: dst.into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
            }],
        }
    }

    #[test]
    fn first_materialization_runs_worktree_add_and_writes_record() {
        let repo = make_repo_root();
        let data = tempfile::TempDir::new().unwrap();
        let container_dir = data.path().join("jackin-the-architect");
        std::fs::create_dir_all(&container_dir).unwrap();

        let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
        // capture queue (in order materialize_workspace will request):
        //   preflight: rev-parse --show-toplevel
        //   preflight: status --porcelain
        //   ensure_worktree_config: extensions.worktreeConfig --get
        //   ensure_worktree_config: core.repositoryformatversion --get
        //   rev-parse HEAD
        let mut runner = fake_with_outputs(&[
            &repo.path().to_string_lossy(), // rev-parse --show-toplevel (preflight)
            "",                             // status --porcelain (clean)
            "",                             // extensions.worktreeConfig --get (not enabled)
            "0",                            // core.repositoryformatversion --get
            "deadbeef\n",                   // rev-parse HEAD
        ]);

        let mat = materialize_workspace(
            &resolved,
            &container_dir,
            "the-architect",
            "jackin-the-architect",
            "jackin",
            &PreflightContext {
                workspace_name: "jackin".into(),
                force: false,
                interactive: false,
            },
            &mut runner,
        )
        .unwrap();

        assert_eq!(mat.mounts.len(), 1);
        let m = &mat.mounts[0];
        assert!(
            m.bind_src
                .contains("/git/worktree/repo/workspace/jackin/jackin-the-architect"),
            "worktree subdir basename = container name, under git/worktree/repo/<dst-tree>/; got {}",
            m.bind_src
        );
        assert_eq!(m.dst, "/workspace/jackin");
        assert_eq!(m.isolation, MountIsolation::Worktree);

        // git worktree add should have been invoked.
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree add"))
        );

        // record persisted; branch follows Model B (container name verbatim).
        let recs = read_records(&container_dir).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].cleanup_status, CleanupStatus::Active);
        assert_eq!(recs[0].base_commit, "deadbeef");
        assert_eq!(recs[0].scratch_branch, "jackin/scratch/jackin-the-architect");
    }

    #[test]
    fn shared_mounts_pass_through_unchanged() {
        let data = tempfile::TempDir::new().unwrap();
        let container_dir = data.path().join("jackin-x");
        std::fs::create_dir_all(&container_dir).unwrap();
        let resolved = ResolvedWorkspace {
            label: "jackin".into(),
            workdir: "/workspace/x".into(),
            mounts: vec![MountConfig {
                src: "/tmp/cache".into(),
                dst: "/workspace/cache".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
        };
        let mut runner = FakeRunner::default();
        let mat = materialize_workspace(
            &resolved,
            &container_dir,
            "x",
            "jackin-x",
            "jackin",
            &PreflightContext {
                workspace_name: "jackin".into(),
                force: false,
                interactive: false,
            },
            &mut runner,
        )
        .unwrap();
        assert_eq!(mat.mounts[0].bind_src, "/tmp/cache");
        assert_eq!(mat.mounts[0].isolation, MountIsolation::Shared);
        assert!(
            runner.run_recorded.is_empty(),
            "no git ops for shared mounts"
        );
    }

    #[test]
    fn second_materialization_with_existing_record_skips_git_ops() {
        let repo = make_repo_root();
        let data = tempfile::TempDir::new().unwrap();
        let container_dir = data.path().join("jackin-x");
        std::fs::create_dir_all(&container_dir).unwrap();

        let dst = "/workspace/jackin";
        let wt_path = worktree_path_for(&container_dir, dst, "jackin-x");
        std::fs::create_dir_all(&wt_path).unwrap();
        std::fs::write(wt_path.join(".git"), "gitdir: /elsewhere").unwrap();
        crate::isolation::state::write_records(
            &container_dir,
            std::slice::from_ref(&crate::isolation::state::IsolationRecord {
                workspace: "jackin".into(),
                mount_dst: dst.into(),
                original_src: repo.path().to_string_lossy().into(),
                isolation: MountIsolation::Worktree,
                worktree_path: wt_path.to_string_lossy().into(),
                scratch_branch: "jackin/scratch/x".into(),
                base_commit: "abc".into(),
                selector_key: "x".into(),
                container_name: "jackin-x".into(),
                cleanup_status: CleanupStatus::Active,
            }),
        )
        .unwrap();

        let resolved = resolved_with_one_isolated(repo.path(), dst);
        let mut runner = FakeRunner::default();
        let mat = materialize_workspace(
            &resolved,
            &container_dir,
            "x",
            "jackin-x",
            "jackin",
            &PreflightContext {
                workspace_name: "jackin".into(),
                force: false,
                interactive: false,
            },
            &mut runner,
        )
        .unwrap();
        assert_eq!(mat.mounts[0].bind_src, wt_path.to_string_lossy());
        assert!(runner.run_recorded.is_empty(), "no git ops on reuse");
    }

    #[test]
    fn drift_when_recorded_src_differs_errors_before_git_ops() {
        let repo = make_repo_root();
        let data = tempfile::TempDir::new().unwrap();
        let container_dir = data.path().join("jackin-x");
        std::fs::create_dir_all(&container_dir).unwrap();

        let dst = "/workspace/jackin";
        let wt_path = worktree_path_for(&container_dir, dst, "jackin-x");
        std::fs::create_dir_all(&wt_path).unwrap();
        crate::isolation::state::write_records(
            &container_dir,
            std::slice::from_ref(&crate::isolation::state::IsolationRecord {
                workspace: "jackin".into(),
                mount_dst: dst.into(),
                original_src: "/different/src".into(),
                isolation: MountIsolation::Worktree,
                worktree_path: wt_path.to_string_lossy().into(),
                scratch_branch: "jackin/scratch/x".into(),
                base_commit: "abc".into(),
                selector_key: "x".into(),
                container_name: "jackin-x".into(),
                cleanup_status: CleanupStatus::Active,
            }),
        )
        .unwrap();

        let resolved = resolved_with_one_isolated(repo.path(), dst);
        let mut runner = FakeRunner::default();
        let err = materialize_workspace(
            &resolved,
            &container_dir,
            "x",
            "jackin-x",
            "jackin",
            &PreflightContext {
                workspace_name: "jackin".into(),
                force: false,
                interactive: false,
            },
            &mut runner,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("source drift") || msg.contains("differs"));
        assert!(msg.contains("/different/src"));
        assert!(runner.run_recorded.is_empty(), "no git ops on drift error");
    }

    // Removed test `two_isolated_mounts_same_repo_get_dst_suffixed_branches` —
    // workspace validation now rejects this case at the layout level
    // (`validate_isolation_layout` rule 2). The corresponding suffix
    // logic in materialize is gone; coverage moves to
    // `workspace::tests::isolation_layout_rejects_two_worktree_mounts_on_same_repo`.

    #[test]
    fn docker_mount_order_is_length_ascending() {
        let mat = MaterializedWorkspace {
            workdir: "/workspace".into(),
            mounts: vec![
                MaterializedMount {
                    bind_src: "/cache".into(),
                    dst: "/workspace/proj/target".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                    worktree_aux: None,
                },
                MaterializedMount {
                    bind_src: "/wt".into(),
                    dst: "/workspace/proj".into(),
                    readonly: false,
                    isolation: MountIsolation::Worktree,
                    worktree_aux: None,
                },
            ],
        };
        let ordered = mount_order_for_docker(&mat);
        assert_eq!(ordered[0].dst, "/workspace/proj");
        assert_eq!(ordered[1].dst, "/workspace/proj/target");
    }

    #[test]
    fn docker_mount_order_is_stable_for_same_length() {
        let mat = MaterializedWorkspace {
            workdir: "/workspace".into(),
            mounts: vec![
                MaterializedMount {
                    bind_src: "/a".into(),
                    dst: "/workspace/aa".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                    worktree_aux: None,
                },
                MaterializedMount {
                    bind_src: "/b".into(),
                    dst: "/workspace/bb".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                    worktree_aux: None,
                },
            ],
        };
        let ordered = mount_order_for_docker(&mat);
        assert_eq!(ordered[0].dst, "/workspace/aa");
        assert_eq!(ordered[1].dst, "/workspace/bb");
    }
}
