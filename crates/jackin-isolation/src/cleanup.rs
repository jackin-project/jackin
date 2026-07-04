// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Force-delete an isolated worktree, scratch branch, and `isolation.json` record.
//!
//! Tolerates idempotent paths (already-removed worktree, already-deleted
//! branch). Bails without removing the record on real failures so the operator
//! can investigate and re-run `jackin purge`. Not responsible for branch-name
//! derivation (`branch.rs`) or record persistence schema (`state.rs`).

#![expect(
    clippy::print_stderr,
    reason = "isolation cleanup emits operator-visible cleanup warnings"
)]

use crate::state::{IsolationRecord, remove_record};
use jackin_core::CommandRunner;
use jackin_diagnostics::debug_log;
use std::path::Path;

/// Force-delete an isolated worktree and its scratch branch, then remove
/// the corresponding `isolation.json` record.
///
/// Tolerates the idempotent paths (worktree already removed externally,
/// branch already deleted, host repo missing) without surfacing them as
/// errors. Real failures (worktree dir still present after both git and
/// `rm -rf`, or scratch branch still present after `branch -D`) bail
/// **without** removing the record so the operator can investigate and
/// re-run `jackin purge` once the underlying issue is resolved. Removing
/// the record on a failed cleanup would leave orphan git admin entries
/// (`git worktree list` showing stale paths) and orphan branches with no
/// jackin-side reference, which can only be reclaimed by manually running
/// `git worktree prune` and `git branch -D` on the host repo.
// verify-and-bail flow has lots of small steps; splitting hurts readability
pub async fn force_cleanup_isolated(
    record: &IsolationRecord,
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    if matches!(record.isolation, crate::MountIsolation::Clone) {
        return force_cleanup_clone(record, container_state_dir);
    }

    let host_repo_exists = Path::new(&record.original_src).exists();
    debug_log!(
        "isolation",
        "force_cleanup_isolated: container={c} mount={d} branch={b} worktree={w} host_repo_exists={exists}",
        c = record.container_name,
        d = record.mount_dst,
        b = record.scratch_branch,
        w = record.worktree_path,
        exists = host_repo_exists,
    );

    if host_repo_exists {
        debug_log!(
            "isolation",
            "git -C {src} worktree remove --force {wt}",
            src = record.original_src,
            wt = record.worktree_path,
        );
        let wt_remove_result = runner
            .run(
                "git",
                &[
                    "-C",
                    &record.original_src,
                    "worktree",
                    "remove",
                    "--force",
                    &record.worktree_path,
                ],
                None,
                &jackin_core::RunOptions {
                    quiet: true,
                    ..Default::default()
                },
            )
            .await;
        if let Err(e) = &wt_remove_result {
            debug_log!(
                "isolation",
                "git worktree remove returned error for {wt}: {e} (verifying via wt.exists())",
                wt = record.worktree_path,
            );
        }
        debug_log!(
            "isolation",
            "git -C {src} branch -D {branch}",
            src = record.original_src,
            branch = record.scratch_branch,
        );
        let branch_delete_result = runner
            .run(
                "git",
                &[
                    "-C",
                    &record.original_src,
                    "branch",
                    "-D",
                    &record.scratch_branch,
                ],
                None,
                &jackin_core::RunOptions {
                    quiet: true,
                    ..Default::default()
                },
            )
            .await;
        if let Err(e) = &branch_delete_result {
            debug_log!(
                "isolation",
                "git branch -D returned error for {branch}: {e} (verifying via branch_still_present())",
                branch = record.scratch_branch,
            );
        }

        // Verify the branch is actually gone. If `branch -D` errored
        // because the branch was already deleted, the verification
        // succeeds and we proceed; if it errored because the branch is
        // still checked out somewhere or we lack permission, the verify
        // fails and we bail without forgetting the record.
        if branch_still_present(runner, &record.original_src, &record.scratch_branch).await
            == Some(true)
        {
            anyhow::bail!(
                "scratch branch `{}` still present after `git branch -D` on host repo `{}`; \
                 record retained at `{}` so re-running `jackin purge` is possible after \
                 resolving the underlying issue (branch may be checked out in another worktree, \
                 or you may lack permission to delete it).",
                record.scratch_branch,
                record.original_src,
                container_state_dir.display(),
            );
        }
    } else {
        debug_log!(
            "isolation",
            "skipping git cleanup: host repo {src} no longer exists",
            src = record.original_src,
        );
        eprintln!(
            "[jackin] warning: host repo `{src}` no longer exists; \
             cannot run git cleanup for `{dst}`. The orphan admin entry under \
             `<host_repo>/.git/worktrees/` will be reclaimed by `git worktree prune` \
             next time you visit the (moved?) host repo.",
            src = record.original_src,
            dst = record.mount_dst,
        );
    }

    // Belt-and-suspenders: nuke the worktree directory if git left
    // anything. Surface fs errors loudly — a failed rm-rf with the
    // worktree still present means cleanup didn't really happen.
    let wt = Path::new(&record.worktree_path);
    if wt.exists() {
        debug_log!(
            "isolation",
            "fallback rm -rf {wt} (git did not remove it)",
            wt = record.worktree_path,
        );
        if let Err(e) = std::fs::remove_dir_all(wt) {
            anyhow::bail!(
                "could not remove worktree directory `{}`: {e}; \
                 record retained at `{}` so re-running `jackin purge` is possible \
                 after resolving the underlying issue (file in use, permission \
                 denied, or filesystem error).",
                record.worktree_path,
                container_state_dir.display(),
            );
        }
    }

    // Final guard: if the worktree path still exists at this point
    // (shouldn't happen given the rm above), bail rather than forget.
    if wt.exists() {
        anyhow::bail!(
            "worktree directory `{}` still present after cleanup; \
             record retained at `{}` so re-running `jackin purge` is possible.",
            record.worktree_path,
            container_state_dir.display(),
        );
    }

    remove_record(container_state_dir, &record.mount_dst)?;
    Ok(())
}

fn force_cleanup_clone(record: &IsolationRecord, container_state_dir: &Path) -> anyhow::Result<()> {
    debug_log!(
        "isolation",
        "force_cleanup_clone: container={c} mount={d} clone={w}",
        c = record.container_name,
        d = record.mount_dst,
        w = record.worktree_path,
    );
    let clone_path = Path::new(&record.worktree_path);
    if clone_path.exists() {
        std::fs::remove_dir_all(clone_path).map_err(|e| {
            anyhow::anyhow!(
                "could not remove clone directory `{}`: {e}; record retained at `{}` so re-running `jackin purge` is possible after resolving the underlying issue",
                record.worktree_path,
                container_state_dir.display(),
            )
        })?;
    }
    if clone_path.exists() {
        anyhow::bail!(
            "clone directory `{}` still present after cleanup; record retained at `{}` so re-running `jackin purge` is possible.",
            record.worktree_path,
            container_state_dir.display(),
        );
    }
    remove_record(container_state_dir, &record.mount_dst)?;
    Ok(())
}

/// Best-effort check: is `branch` still present on `repo`? Returns
/// `Some(true)` if confirmed present, `Some(false)` if confirmed absent,
/// `None` if we couldn't tell (e.g., `git branch --list` itself errored).
/// Callers treat `None` as "couldn't verify, don't bail" — the cost of
/// a false negative here (orphan branch left behind) is much lower than
/// the cost of a false positive (operator stuck unable to purge).
async fn branch_still_present(
    runner: &mut impl CommandRunner,
    repo: &str,
    branch: &str,
) -> Option<bool> {
    let output = runner
        .capture("git", &["-C", repo, "branch", "--list", branch], None)
        .await
        .ok()?;
    Some(!output.trim().is_empty())
}

/// Force-cleanup every record in a container's isolation.json. Used by purge.
///
/// Iterates ALL records (does not stop at the first failure) so a single
/// stuck mount doesn't block cleanup of independent siblings. After the
/// loop, if any record failed to clean, surfaces an aggregate `Err` so
/// the caller's exit code reflects reality — operator gets a non-zero
/// status and an actionable summary instead of a misleading exit-0
/// "purge succeeded" with a warning that scrolled past.
pub async fn purge_isolated_for_container(
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let records = crate::state::read_records(container_state_dir)?;
    debug_log!(
        "isolation",
        "purge_isolated_for_container: {n} record(s) under {dir}",
        n = records.len(),
        dir = container_state_dir.display(),
    );
    let mut failed: Vec<String> = Vec::new();
    for rec in records {
        if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner).await {
            eprintln!(
                "[jackin] warning: failed to clean up isolated mount `{}`: {e}",
                rec.mount_dst
            );
            failed.push(rec.mount_dst);
        }
    }
    if !failed.is_empty() {
        anyhow::bail!(
            "purge of isolated mounts had {n} failure(s): {list}; \
             record(s) retained at `{dir}` so re-running `jackin purge` is possible \
             after resolving the underlying issue(s) (see warnings above for details)",
            n = failed.len(),
            list = failed.join(", "),
            dir = container_state_dir.display(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
