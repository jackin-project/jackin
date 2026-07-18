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

    if host_repo_exists {
        drop(
            runner
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
                .await,
        );
        drop(
            runner
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
                .await,
        );

        // Verify the branch is actually gone. If `branch -D` errored
        // because the branch was already deleted, the verification
        // succeeds and we proceed; if it errored because the branch is
        // still checked out somewhere or we lack permission, the verify
        // fails and we bail without forgetting the record.
        if branch_still_present(runner, &record.original_src, &record.scratch_branch).await
            == Some(true)
        {
            return Err(crate::IsolationError::ScratchBranchRemains {
                branch: record.scratch_branch.clone(),
                repo: record.original_src.clone(),
                state_dir: container_state_dir.to_path_buf(),
            }
            .into());
        }
    } else {
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
    if wt.exists()
        && let Err(e) = std::fs::remove_dir_all(wt)
    {
        return Err(crate::IsolationError::WorktreeRemove {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
            source: e,
        }
        .into());
    }

    // Final guard: if the worktree path still exists at this point
    // (shouldn't happen given the rm above), bail rather than forget.
    if wt.exists() {
        return Err(crate::IsolationError::WorktreeStillPresent {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
        }
        .into());
    }

    remove_record(container_state_dir, &record.mount_dst)?;
    Ok(())
}

fn force_cleanup_clone(record: &IsolationRecord, container_state_dir: &Path) -> anyhow::Result<()> {
    let clone_path = Path::new(&record.worktree_path);
    if clone_path.exists() {
        std::fs::remove_dir_all(clone_path).map_err(|e| crate::IsolationError::CloneRemove {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
            source: e,
        })?;
    }
    if clone_path.exists() {
        return Err(crate::IsolationError::CloneStillPresent {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
        }
        .into());
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
        return Err(crate::IsolationError::PurgePartialFailure {
            n: failed.len(),
            list: failed.join(", "),
            state_dir: container_state_dir.to_path_buf(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
