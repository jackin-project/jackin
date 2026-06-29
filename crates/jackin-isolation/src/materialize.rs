//! Mount materialization: converts `WorkspaceConfig` mounts into concrete
//! Docker bind-mount specs, cloning worktrees for isolated mounts and writing
//! the `IsolationRecord` consumed by the finalizer.
//!
//! Produces a `MaterializedWorkspace` whose `mounts` list is ready for
//! `docker run --mount`. Isolated mounts additionally carry `worktree_aux`
//! bind entries so git resolves the gitdir relationship correctly inside the
//! container (see `WorktreeAuxMounts` for the bind topology).
//!
//! Not responsible for: finalization or cleanup of existing worktrees
//! (`isolation/finalize.rs` and `isolation/cleanup.rs`).

use crate::MountIsolation;
use crate::branch::branch_name;
use crate::state::{CleanupStatus, IsolationRecord, read_record, upsert_record};
use anyhow::Context;
use jackin_config::ResolvedWorkspace;
use jackin_core::CommandRunner;
use jackin_diagnostics::debug_log;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedWorkspace {
    pub workdir: String,
    pub mounts: Vec<MaterializedMount>,
    /// Threaded through from `ResolvedWorkspace` so `launch_role_runtime`
    /// can stamp the `jackin.keep.awake=true` label on the container
    /// without a config re-lookup. Read by the keep-awake reconciler.
    pub keep_awake_enabled: bool,
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
/// the role's gitdir resolves to a container path), one shadowing the
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
/// as git-managed and `overrides/` holding jackin❯'s bind-mount sources):
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

/// Compute the host-side clone path for an isolated mount.
///
/// Clone mode stores a full per-agent repository under the same
/// per-container `git/` tree as worktree mode, but uses a separate
/// top-level discriminator so cleanup and manual inspection are obvious:
///
/// ```text
/// <container_state>/git/clone/repo/<dst-stripped>/<container>/
/// ```
pub fn clone_path_for(container_state_dir: &Path, dst: &str, container_name: &str) -> PathBuf {
    let rel = dst.trim_matches('/');
    container_state_dir
        .join("git")
        .join("clone")
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

/// Mirror of `jackin_runtime::runtime::repo_cache::normalize_github_url`.
/// Inlined here (instead of pulling in a runtime dep that would
/// cycle) because the function is 10 lines and only used at one
/// call site in this crate. Keeps jackin-isolation's allowed-deps
/// list (jackin-core / jackin-config / jackin-diagnostics) clean
/// per the Architecture-Invariant header in lib.rs.
fn normalize_github_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return format!("https://github.com/{rest}");
    }
    if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        return format!("https://github.com/{rest}");
    }
    url.to_owned()
}

/// Drop an embedded `userinfo@` from an HTTP(S) URL so a host-side
/// PAT in the operator's `origin` does not get copied into the
/// per-container clone's `.git/config`. SCP / `ssh://` forms pass
/// through unchanged — leading `git@` is an SSH identity, not a
/// credential.
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

/// Write the two jackin-owned override files alongside the materialized
/// worktree. Idempotent: rewrites both files on every call so reused
/// worktrees pick up any topology changes (rare, but cheap).
///
/// Storage layout: every git-related artifact for one mount lives
/// under `<container_state>/git/`, with `worktree/repo/` marking the
/// subtree as git-managed and `overrides/` holding jackin❯'s
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
        .await
        .unwrap_or_default();
    if format_version.trim() == "0" || format_version.trim().is_empty() {
        debug_log!(
            "isolation",
            "bumping core.repositoryformatversion 0 -> 1 at {} (required for extensions.worktreeConfig)",
            repo.display()
        );
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
    debug_log!(
        "isolation",
        "enabling extensions.worktreeConfig at {} (per-worktree config from now on)",
        repo.display()
    );
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

/// Filesystem probe (loose ref then `packed-refs`) rather than `git
/// show-ref` to keep the test `CommandRunner` capture queue stable.
fn find_local_branch_tip(repo: &str, branch: &str) -> Option<String> {
    let git_dir = Path::new(repo).join(".git");
    let mut loose = git_dir.join("refs").join("heads");
    for segment in branch.split('/') {
        loose = loose.join(segment);
    }
    match std::fs::read_to_string(&loose) {
        Ok(contents) => {
            let sha = contents.trim();
            // Symref content (`ref: refs/heads/foo`) and a 0-byte
            // ref file are pathological local states that would
            // otherwise be returned verbatim and poison the
            // `IsolationRecord.base_commit` SHA field. Fall through
            // to packed-refs and (failing that) `None` so the caller
            // takes the fresh `-b` path; git will surface its own
            // error if the branch genuinely does exist somewhere
            // unreadable.
            if sha.is_empty() {
                debug_log!(
                    "isolation",
                    "find_local_branch_tip: loose ref {loose} present but empty — treating as missing",
                    loose = loose.display(),
                );
            } else if sha.starts_with("ref:") {
                debug_log!(
                    "isolation",
                    "find_local_branch_tip: loose ref {loose} is a symref ({sha}) — treating as missing",
                    loose = loose.display(),
                );
            } else {
                return Some(sha.to_owned());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            // Permission / EIO / non-UTF-8: surface so --debug
            // correlates with the later worktree-add failure.
            debug_log!(
                "isolation",
                "find_local_branch_tip: read {loose} failed: {e}",
                loose = loose.display(),
            );
        }
    }
    let packed = git_dir.join("packed-refs");
    let contents = match std::fs::read_to_string(&packed) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            debug_log!(
                "isolation",
                "find_local_branch_tip: read {packed} failed: {e}",
                packed = packed.display(),
            );
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
    pub workspace_name: String,
    pub force: bool,
    pub interactive: bool,
}

/// Validation that must pass before host-side git materialization. Layout
/// validation (parent/child rejection) happens earlier at config-validation
/// time; this is per-mount.
pub async fn preflight_isolated(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    // readonly is incompatible with isolated editable source modes.
    anyhow::ensure!(
        !mount.readonly,
        "isolated mount `{}` cannot be readonly (isolation = {})",
        mount.dst,
        mount.isolation
    );

    // Sensitive mount overlap.
    let sensitives = jackin_config::find_sensitive_mounts(std::slice::from_ref(mount));
    if let Some(s) = sensitives.first() {
        anyhow::bail!(
            "isolated mount `{}` overlaps sensitive path `{}` ({}) (isolation = {})",
            mount.dst,
            s.src,
            s.reason,
            mount.isolation,
        );
    }

    let src = Path::new(&mount.src);

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
    anyhow::ensure!(
        src_canon == top_canon,
        "isolated mount `{}`: src `{}` is inside repo `{}` but not its root",
        mount.dst,
        mount.src,
        toplevel
    );

    // Dirty tree check (separate test in 4.5).
    check_dirty_tree(mount, ctx, runner).await?;

    Ok(())
}

pub async fn preflight_worktree(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    preflight_isolated(mount, ctx, runner).await
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
pub async fn materialize_workspace(
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
        .filter(|m| !m.isolation.is_shared())
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

    // Re-emit in original order — Docker mount-flag order is settled later.
    let mounts: Vec<MaterializedMount> = materialized
        .into_iter()
        .collect::<Option<_>>()
        .ok_or_else(|| {
            anyhow::anyhow!("internal mount materialization error: missing mount slot")
        })?;
    Ok(MaterializedWorkspace {
        workdir: resolved.workdir.clone(),
        mounts,
        keep_awake_enabled: resolved.keep_awake_enabled,
    })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn materialize_one(
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
                "source drift on container `{}`, mount `{}`: recorded src `{}` differs from configured src `{}`; preserved {} at `{}`. Restore the previous src, inspect the path above, or `jackin purge {}` to discard.",
                container_name,
                mount.dst,
                record.original_src,
                mount.src,
                record.isolation,
                record.worktree_path,
                container_name,
            );
        }
        if record.isolation != MountIsolation::Worktree {
            anyhow::bail!(
                "isolation mode drift on container `{}`, mount `{}`: recorded mode `{}` differs from configured mode `{}`; preserved {} at `{}`. Run `jackin purge {}` to discard the old isolated state before switching modes.",
                container_name,
                mount.dst,
                record.isolation,
                mount.isolation,
                record.isolation,
                record.worktree_path,
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
    preflight_worktree(mount, ctx, runner).await?;

    let _ = ensure_worktree_config_enabled(Path::new(&mount.src), runner).await?;

    let host_head = runner
        .capture("git", &["-C", &mount.src, "rev-parse", "HEAD"], None)
        .await?
        .trim()
        .to_owned();
    debug_log!(
        "isolation",
        "mount {dst}: host HEAD {commit}",
        dst = mount.dst,
        commit = host_head,
    );

    // No per-mount branch suffix in V1: workspace validation rejects
    // two isolated mounts on the same host repo (see
    // `validate_isolation_layout`), so each container has at most one
    // isolated mount per host repo and the scratch branch is uniquely
    // named by the selector alone.
    // Branch name = `jackin/scratch/<container>` (Model B). Container
    // name is the disambiguator because it's globally unique by jackin
    // construction; selector alone wouldn't disambiguate parallel
    // containers of the same role class (which would collide on the
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

    // `worktree add -b` rejects an existing branch; reuse it. Record
    // `base_commit = host_head`, NOT branch tip: if a prior session
    // committed work onto the scratch branch and the operator wiped
    // the state dir, branch tip != host HEAD, and finalize routes
    // through the upstream/[gone]/detached arms (fail-safe to
    // PreservedUnpushed) instead of the `tip == base_commit ⇒ Safe`
    // arm that would silently delete the work.
    let base_commit = if let Some(branch_tip) = find_local_branch_tip(&mount.src, &scratch_branch) {
        debug_log!(
            "isolation",
            "mount {dst}: adopting existing scratch branch {branch} at tip {tip} (host HEAD {host}); pruning orphan admin entries first",
            dst = mount.dst,
            branch = scratch_branch,
            tip = branch_tip,
            host = host_head,
        );
        runner
            .run(
                "git",
                &["-C", &mount.src, "worktree", "prune"],
                None,
                &jackin_core::RunOptions::default(),
            )
            .await?;
        debug_log!(
            "isolation",
            "mount {dst}: git -C {src} worktree add {wt} {branch}",
            dst = mount.dst,
            src = mount.src,
            branch = scratch_branch,
            wt = worktree_path.display(),
        );
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
        debug_log!(
            "isolation",
            "mount {dst}: git -C {src} worktree add -b {branch} {wt} {base}",
            dst = mount.dst,
            src = mount.src,
            branch = scratch_branch,
            wt = worktree_path.display(),
            base = host_head,
        );
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

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn materialize_clone(
    mount: &MountConfig,
    container_state_dir: &Path,
    selector_key: &str,
    container_name: &str,
    workspace_name: &str,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<MaterializedMount> {
    let clone_path = clone_path_for(container_state_dir, &mount.dst, container_name);
    debug_log!(
        "isolation",
        "mount {dst}: clone (src={src} → clone_path={cp})",
        dst = mount.dst,
        src = mount.src,
        cp = clone_path.display(),
    );

    if let Some(record) = read_record(container_state_dir, &mount.dst)? {
        if record.original_src != mount.src {
            anyhow::bail!(
                "source drift on container `{}`, mount `{}`: recorded src `{}` differs from configured src `{}`; preserved {} at `{}`. Restore the previous src, inspect the path above, or `jackin purge {}` to discard.",
                container_name,
                mount.dst,
                record.original_src,
                mount.src,
                record.isolation,
                record.worktree_path,
                container_name,
            );
        }
        if record.isolation != MountIsolation::Clone {
            anyhow::bail!(
                "isolation mode drift on container `{}`, mount `{}`: recorded mode `{}` differs from configured mode `{}`; preserved {} at `{}`. Run `jackin purge {}` to discard the old isolated state before switching modes.",
                container_name,
                mount.dst,
                record.isolation,
                mount.isolation,
                record.isolation,
                record.worktree_path,
                container_name,
            );
        }
        if clone_path.join(".git").exists() {
            debug_log!(
                "isolation",
                "mount {dst}: reusing existing clone (record matches and .git present)",
                dst = mount.dst,
            );
            return Ok(MaterializedMount {
                bind_src: clone_path.to_string_lossy().into(),
                dst: mount.dst.clone(),
                readonly: mount.readonly,
                isolation: MountIsolation::Clone,
                worktree_aux: None,
            });
        }
        debug_log!(
            "isolation",
            "mount {dst}: record present but clone directory missing — recreating",
            dst = mount.dst,
        );
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

    // `git clone --local <mount.src>` points the clone's `origin` at
    // `mount.src` — on jackin❯'s mount layout that path is identical
    // inside and outside the container, so pushes loop back to a
    // host-local working tree instead of the operator's upstream.
    // Copy the host's own `origin` URL across (worktree mode inherits
    // it via shared `.git/config`; clone mode has to do it by hand).
    // GitHub SCP / `ssh://` forms run through `normalize_github_url`
    // so the container's `gh` credential helper can authenticate
    // without an SSH key; embedded `userinfo@` credentials are
    // stripped so a host-side PAT does not leak into the per-container
    // `.git/config`.
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
            // `error: No such remote 'origin'` (fresh init, never
            // pushed) is a legitimate fall-through — agent has no
            // upstream anyway. Anything else (permission denied,
            // corrupt config) aborts the launch; silently landing on
            // a loopback origin would misroute the operator's pushes.
            let chain = format!("{err:#}");
            if chain.contains("No such remote") || chain.contains("no such remote") {
                debug_log!(
                    "isolation",
                    "mount {dst}: host repo has no `origin` remote; leaving clone origin at bind-mount path",
                    dst = mount.dst,
                );
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
        debug_log!(
            "isolation",
            "mount {dst}: rewriting clone origin from {src} to {url}",
            dst = mount.dst,
            src = mount.src,
            url = url,
        );
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
    } else {
        debug_log!(
            "isolation",
            "mount {dst}: host repo has no origin remote — leaving clone origin at bind-mount path",
            dst = mount.dst,
        );
    }

    upsert_record(
        container_state_dir,
        IsolationRecord {
            workspace: workspace_name.into(),
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

/// Order mounts so parents appear before children. Docker overlays later
/// mounts on earlier ones, so this lets a shared cache child mount land
/// inside an isolated worktree parent.
pub fn mount_order_for_docker(mat: &MaterializedWorkspace) -> Vec<&MaterializedMount> {
    let mut ordered: Vec<&MaterializedMount> = mat.mounts.iter().collect();
    ordered.sort_by_key(|m| m.dst.trim_end_matches('/').len());
    ordered
}

#[cfg(test)]
mod tests;
