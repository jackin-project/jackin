use crate::debug_log;
use crate::docker::CommandRunner;
use crate::isolation::MountIsolation;
use crate::isolation::branch::{branch_name, dst_to_branch_suffix};
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

/// Three extra bind mounts the container needs so a worktree's
/// gitdir relationship resolves consistently inside the container.
///
/// All three sources are jackin-owned (under the container's state
/// dir or the host repo's `.git/`); host-side worktree files are
/// never modified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeAuxMounts {
    /// Host source: `<host_repo>/.git`. Container target:
    /// `/jackin-isolation/<container>-git`. Read-write — git needs to
    /// write refs/HEAD/etc on agent commits.
    pub host_git_dir: String,
    pub host_git_target: String,
    /// Host source: jackin-owned override `.git` file containing
    /// `gitdir: <host_git_target>/worktrees/<wt-name>`. Container
    /// target: `<dst>/.git`. Overrides the worktree's host-side `.git`
    /// pointer file with one resolvable inside the container.
    pub git_file_override: String,
    pub git_file_target: String,
    /// Host source: jackin-owned override file containing `<dst>/.git`.
    /// Container target:
    /// `<host_git_target>/worktrees/<wt-name>/gitdir`. Overrides git's
    /// admin-dir back-pointer so its verification check (back-pointer
    /// must match the worktree's `.git` location) passes inside the
    /// container, where `<dst>` differs from the host worktree path.
    pub gitdir_back_override: String,
    pub gitdir_back_target: String,
}

/// Compute the host-side worktree path for an isolated mount.
/// Strips leading and trailing `/` from `dst` so the path is relative
/// when joined under `<container_state_dir>/isolated/`.
pub fn worktree_path_for(container_state_dir: &Path, dst: &str) -> PathBuf {
    let rel = dst.trim_matches('/');
    container_state_dir.join("isolated").join(rel)
}

/// Stable container-side path where the host repo's `.git/` is bind-
/// mounted for an isolated worktree. Hardcoded prefix; the variable
/// part is the container name so concurrent agents don't collide.
fn container_git_dir_path(container_name: &str) -> String {
    format!("/jackin-isolation/{container_name}-git")
}

/// Filesystem-safe identifier derived from `dst` for naming override
/// files. `/workspace/jackin` → `workspace_jackin`. Strip leading and
/// trailing `/` first so the result has no awkward leading underscore.
fn override_id_for_dst(dst: &str) -> String {
    dst.trim_matches('/').replace('/', "_")
}

/// Write the two jackin-owned override files alongside the materialized
/// worktree. Idempotent: rewrites both files on every call so reused
/// worktrees pick up any topology changes (rare, but cheap).
///
/// Returns the [`WorktreeAuxMounts`] needed to wire up the three
/// auxiliary bind mounts at docker-run time.
fn write_git_overrides(
    container_state_dir: &Path,
    container_name: &str,
    mount_dst: &str,
    worktree_path: &Path,
    host_repo_src: &str,
) -> anyhow::Result<WorktreeAuxMounts> {
    let overrides_dir = container_state_dir.join(".git-overrides");
    std::fs::create_dir_all(&overrides_dir)
        .with_context(|| format!("create git-overrides dir at {}", overrides_dir.display()))?;

    let wt_name = worktree_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!("worktree path has no basename: {}", worktree_path.display())
        })?;
    let id = override_id_for_dst(mount_dst);
    let host_git_target = container_git_dir_path(container_name);

    // Override 1: replacement `.git` text file at `<dst>/.git` inside
    // the container. Points to the container-side gitdir admin path.
    let git_file_override_path = overrides_dir.join(format!("{id}.git-file"));
    let git_file_content = format!("gitdir: {host_git_target}/worktrees/{wt_name}\n");
    std::fs::write(&git_file_override_path, &git_file_content).with_context(|| {
        format!(
            "write git-file override {}",
            git_file_override_path.display()
        )
    })?;

    // Override 2: replacement back-pointer at
    // `<host_git_target>/worktrees/<wt_name>/gitdir` inside the
    // container. Tells git "the worktree's `.git` file is at <dst>/.git"
    // so its verification check passes (the host's absolute path stored
    // in the on-disk gitdir back-pointer would NOT match `<dst>` inside
    // the container, hence the override).
    let gitdir_back_override_path = overrides_dir.join(format!("{id}.gitdir-back"));
    let gitdir_back_content = format!("{mount_dst}/.git\n");
    std::fs::write(&gitdir_back_override_path, &gitdir_back_content).with_context(|| {
        format!(
            "write gitdir-back override {}",
            gitdir_back_override_path.display()
        )
    })?;

    let host_git_dir = format!("{host_repo_src}/.git");
    let git_file_target = format!("{mount_dst}/.git");
    let gitdir_back_target = format!("{host_git_target}/worktrees/{wt_name}/gitdir");

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

    // Count isolated mounts per host repo for branch disambiguation.
    let isolated_per_repo = count_isolated_per_repo(&resolved.mounts);

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
                &isolated_per_repo,
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

fn canonicalize_or_clone(src: &str) -> String {
    std::fs::canonicalize(src).map_or_else(|_| src.to_owned(), |p| p.to_string_lossy().into_owned())
}

fn count_isolated_per_repo(
    mounts: &[MountConfig],
) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for m in mounts {
        if matches!(m.isolation, MountIsolation::Worktree) {
            let canon = canonicalize_or_clone(&m.src);
            map.entry(canon).or_default().push(m.dst.clone());
        }
    }
    map
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn materialize_one(
    mount: &MountConfig,
    container_state_dir: &Path,
    selector_key: &str,
    container_name: &str,
    workspace_name: &str,
    isolated_per_repo: &std::collections::HashMap<String, Vec<String>>,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<MaterializedMount> {
    let worktree_path = worktree_path_for(container_state_dir, &mount.dst);
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
            let aux = write_git_overrides(
                container_state_dir,
                container_name,
                &mount.dst,
                &worktree_path,
                &mount.src,
            )?;
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

    // Decide branch suffix: only when >1 isolated mount targets the same host repo.
    let canon = canonicalize_or_clone(&mount.src);
    let suffix = isolated_per_repo
        .get(&canon)
        .filter(|dsts| dsts.len() > 1)
        .map(|_| dst_to_branch_suffix(&mount.dst));
    let scratch_branch = branch_name(selector_key, suffix.as_deref());
    debug_log!(
        "isolation",
        "mount {dst}: scratch branch {branch} (selector={selector}, suffix={suffix})",
        dst = mount.dst,
        branch = scratch_branch,
        selector = selector_key,
        suffix = suffix.as_deref().unwrap_or("<none>"),
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

    let aux = write_git_overrides(
        container_state_dir,
        container_name,
        &mount.dst,
        &worktree_path,
        &mount.src,
    )?;
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
    fn worktree_path_strips_leading_slash() {
        let base = PathBuf::from("/data/jackin-x");
        assert_eq!(
            worktree_path_for(&base, "/workspace/jackin"),
            PathBuf::from("/data/jackin-x/isolated/workspace/jackin")
        );
    }

    #[test]
    fn worktree_path_strips_trailing_slash() {
        let base = PathBuf::from("/data/jackin-x");
        assert_eq!(
            worktree_path_for(&base, "/workspace/jackin/"),
            PathBuf::from("/data/jackin-x/isolated/workspace/jackin")
        );
    }

    #[test]
    fn override_id_strips_slashes_and_trims() {
        assert_eq!(override_id_for_dst("/workspace/jackin"), "workspace_jackin");
        assert_eq!(
            override_id_for_dst("/workspace/jackin/"),
            "workspace_jackin"
        );
        assert_eq!(override_id_for_dst("//a/b//"), "a_b");
        assert_eq!(override_id_for_dst("/"), "");
    }

    #[test]
    fn container_git_dir_path_namespaces_by_container_name() {
        // Two parallel agents must not share the host-.git mount path.
        assert_eq!(
            container_git_dir_path("jackin-the-architect"),
            "/jackin-isolation/jackin-the-architect-git"
        );
        assert_eq!(
            container_git_dir_path("jackin-the-architect-clone-1"),
            "/jackin-isolation/jackin-the-architect-clone-1-git"
        );
    }

    #[test]
    fn write_git_overrides_writes_both_files_with_correct_content() {
        let cdir = tempfile::TempDir::new().unwrap();
        let wt = cdir.path().join("isolated/workspace/jackin");
        std::fs::create_dir_all(&wt).unwrap();

        let aux = write_git_overrides(
            cdir.path(),
            "jackin-the-architect",
            "/workspace/jackin",
            &wt,
            "/host/jackin",
        )
        .unwrap();

        // Auxiliary mount metadata reflects the design doc topology.
        assert_eq!(aux.host_git_dir, "/host/jackin/.git");
        assert_eq!(
            aux.host_git_target,
            "/jackin-isolation/jackin-the-architect-git"
        );
        assert_eq!(aux.git_file_target, "/workspace/jackin/.git");
        assert_eq!(
            aux.gitdir_back_target,
            "/jackin-isolation/jackin-the-architect-git/worktrees/jackin/gitdir"
        );

        // File content matches the docs in the roadmap MDX.
        let git_file = std::fs::read_to_string(&aux.git_file_override).unwrap();
        assert_eq!(
            git_file,
            "gitdir: /jackin-isolation/jackin-the-architect-git/worktrees/jackin\n"
        );
        let gitdir_back = std::fs::read_to_string(&aux.gitdir_back_override).unwrap();
        assert_eq!(gitdir_back, "/workspace/jackin/.git\n");

        // Override files live under .git-overrides with the dst-derived id.
        assert!(aux.git_file_override.ends_with("workspace_jackin.git-file"));
        assert!(
            aux.gitdir_back_override
                .ends_with("workspace_jackin.gitdir-back")
        );
    }

    #[test]
    fn write_git_overrides_is_idempotent() {
        let cdir = tempfile::TempDir::new().unwrap();
        let wt = cdir.path().join("isolated/workspace/jackin");
        std::fs::create_dir_all(&wt).unwrap();

        let first = write_git_overrides(
            cdir.path(),
            "jackin-x",
            "/workspace/jackin",
            &wt,
            "/host/jackin",
        )
        .unwrap();
        let second = write_git_overrides(
            cdir.path(),
            "jackin-x",
            "/workspace/jackin",
            &wt,
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
        assert!(m.bind_src.contains("isolated/workspace/jackin"));
        assert_eq!(m.dst, "/workspace/jackin");
        assert_eq!(m.isolation, MountIsolation::Worktree);

        // git worktree add should have been invoked.
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree add"))
        );

        // record persisted
        let recs = read_records(&container_dir).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].cleanup_status, CleanupStatus::Active);
        assert_eq!(recs[0].base_commit, "deadbeef");
        assert_eq!(recs[0].scratch_branch, "jackin/scratch/the-architect");
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
        let wt_path = worktree_path_for(&container_dir, dst);
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
        let wt_path = worktree_path_for(&container_dir, dst);
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

    #[test]
    fn two_isolated_mounts_same_repo_get_dst_suffixed_branches() {
        let repo = make_repo_root();
        let data = tempfile::TempDir::new().unwrap();
        let container_dir = data.path().join("jackin-x");
        std::fs::create_dir_all(&container_dir).unwrap();

        let resolved = ResolvedWorkspace {
            label: "jackin".into(),
            workdir: "/workspace/jackin".into(),
            mounts: vec![
                MountConfig {
                    src: repo.path().to_string_lossy().into(),
                    dst: "/workspace/jackin".into(),
                    readonly: false,
                    isolation: MountIsolation::Worktree,
                },
                MountConfig {
                    src: repo.path().to_string_lossy().into(),
                    dst: "/workspace/jackin-v2".into(),
                    readonly: false,
                    isolation: MountIsolation::Worktree,
                },
            ],
        };

        // Capture order per mount (each mount goes through preflight + ensure):
        // Mount 1 (shorter dst materialized first): rev-parse --show-toplevel,
        //   status --porcelain, ext.worktreeConfig --get, format --get,
        //   rev-parse HEAD
        // Mount 2: same sequence (worktreeConfig will read "true" now)
        let mut runner = fake_with_outputs(&[
            // mount 1
            &repo.path().to_string_lossy(),
            "",
            "",
            "0",
            "abc\n",
            // mount 2 (worktree config now enabled)
            &repo.path().to_string_lossy(),
            "",
            "true\n",
            "abc\n",
        ]);

        let mat = materialize_workspace(
            &resolved,
            &container_dir,
            "the-architect",
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

        // Inspect persisted records for branch names.
        let recs = read_records(&container_dir).unwrap();
        let mut branches: Vec<String> = recs.iter().map(|r| r.scratch_branch.clone()).collect();
        branches.sort();
        assert_eq!(
            branches,
            vec![
                "jackin/scratch/the-architect-workspace-jackin",
                "jackin/scratch/the-architect-workspace-jackin-v2",
            ]
        );

        let _ = mat;
    }

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
