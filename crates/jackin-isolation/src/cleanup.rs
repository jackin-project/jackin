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
use std::collections::HashSet;
use std::path::Path;

/// Force-delete an isolated worktree and its scratch branch, then remove
/// the corresponding `isolation.json` record.
///
/// Treats already-removed worktrees and branches as idempotent only after Git
/// inventory confirms their exact absence. If a command fails and the follow-up
/// inventory is present or inconclusive, the isolation record remains so the
/// operator can investigate and re-run `jackin purge`.
// verify-and-bail flow has lots of small steps; splitting hurts readability
pub async fn force_cleanup_isolated(
    record: &IsolationRecord,
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    if matches!(record.isolation, crate::MountIsolation::Clone) {
        return force_cleanup_clone(record, container_state_dir);
    }

    if !Path::new(&record.original_src).exists() {
        anyhow::bail!(
            "host repo `{}` is missing; cannot verify Git worktree/branch cleanup for `{}`; \
             isolation record retained at `{}`",
            record.original_src,
            record.mount_dst,
            container_state_dir.display(),
        );
    }

    let worktree_remove = runner
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
    let worktree_registered =
        worktree_is_registered(runner, &record.original_src, &record.worktree_path)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "could not verify Git worktree registration for `{}`: {error:#}; \
             isolation record retained at `{}`",
                    record.worktree_path,
                    container_state_dir.display(),
                )
            })?;
    match (worktree_remove, worktree_registered) {
        (Ok(()), false) => {}
        // The command may fail for an already-removed worktree. Accept that
        // idempotent result only because the independent registry query proved
        // this exact path is absent.
        (Err(_), false) => {}
        (Ok(()), true) => anyhow::bail!(
            "Git worktree registration for `{}` remains after removal; \
             isolation record retained at `{}`",
            record.worktree_path,
            container_state_dir.display(),
        ),
        (Err(error), true) => anyhow::bail!(
            "git worktree remove failed: {error:#}; Git worktree registration for `{}` \
             remains; isolation record retained at `{}`",
            record.worktree_path,
            container_state_dir.display(),
        ),
    }

    let branch_remove = runner
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
    let branch_present = branch_still_present(runner, &record.original_src, &record.scratch_branch)
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "could not verify scratch branch `{}` was removed: {error:#}; \
                 isolation record retained at `{}`",
                record.scratch_branch,
                container_state_dir.display(),
            )
        })?;
    match (branch_remove, branch_present) {
        (Ok(()), false) => {}
        // An already-deleted branch is idempotent only after branch listing
        // independently confirms that the exact branch is gone.
        (Err(_), false) => {}
        (Ok(()), true) => {
            return Err(crate::IsolationError::ScratchBranchRemains {
                branch: record.scratch_branch.clone(),
                repo: record.original_src.clone(),
                state_dir: container_state_dir.to_path_buf(),
            }
            .into());
        }
        (Err(error), true) => {
            return Err(anyhow::anyhow!(
                "git branch -D failed: {error:#}; {}",
                crate::IsolationError::ScratchBranchRemains {
                    branch: record.scratch_branch.clone(),
                    repo: record.original_src.clone(),
                    state_dir: container_state_dir.to_path_buf(),
                }
            ));
        }
    }

    // Belt-and-suspenders: nuke the worktree directory if git left
    // anything. The record path is untrusted state, so removal is
    // containment-bound to the container state dir and fd-pinned
    // (`O_NOFOLLOW` at every level): a swapped parent or a symlink in
    // place of the worktree is refused loudly instead of followed.
    // Surface fs errors loudly — a failed rm-rf with the
    // worktree still present means cleanup didn't really happen.
    let wt = Path::new(&record.worktree_path);
    if let Err(e) = crate::safe_remove::safe_remove_dir_contained(container_state_dir, wt) {
        return Err(crate::IsolationError::WorktreeRemove {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
            source: e,
        }
        .into());
    }

    // Final guard: if the worktree path still exists at this point
    // (shouldn't happen given the rm above), bail rather than forget.
    // `symlink_metadata` (not `exists`) so a dangling or hostile symlink
    // left behind is still caught.
    if wt.symlink_metadata().is_ok() {
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
    // Same owned-validated-path removal as the worktree path: the record
    // path is untrusted, so containment-bound fd-pinned deletion refuses
    // escapes and symlinks instead of following them.
    crate::safe_remove::safe_remove_dir_contained(container_state_dir, clone_path).map_err(
        |e| crate::IsolationError::CloneRemove {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
            source: e,
        },
    )?;
    if clone_path.symlink_metadata().is_ok() {
        return Err(crate::IsolationError::CloneStillPresent {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
        }
        .into());
    }
    remove_record(container_state_dir, &record.mount_dst)?;
    Ok(())
}

/// Check whether the exact worktree path still appears in Git's registry.
/// `-z` makes path comparison unambiguous even when a path contains whitespace.
async fn worktree_is_registered(
    runner: &mut impl CommandRunner,
    repo: &str,
    worktree_path: &str,
) -> anyhow::Result<bool> {
    let output = runner
        .capture(
            "git",
            &["-C", repo, "worktree", "list", "--porcelain", "-z"],
            None,
        )
        .await?;
    worktree_inventory_contains_path(&output, worktree_path)
}

/// Parse Git's complete `worktree list --porcelain -z` inventory and report
/// whether it contains the exact path. Git emits the main worktree first,
/// separates records with an empty NUL field, and NUL-terminates the final
/// record too. Treat malformed or lossy output as inconclusive: cleanup must
/// not interpret a partial inventory as proof that a worktree is absent.
fn worktree_inventory_contains_path(output: &str, expected_path: &str) -> anyhow::Result<bool> {
    if output.is_empty() {
        anyhow::bail!("Git worktree inventory is empty");
    }
    if !output.ends_with("\0\0") {
        anyhow::bail!("Git worktree inventory is not terminated at a record boundary");
    }
    // CommandRunner returns UTF-8 text using a lossy conversion. If Git emitted
    // invalid path bytes, U+FFFD makes exact byte comparison impossible.
    if output.contains('\u{FFFD}') {
        anyhow::bail!("Git worktree inventory contains lossy path text");
    }

    let records = output.split("\0\0").collect::<Vec<_>>();
    if records.last() != Some(&"") {
        anyhow::bail!("Git worktree inventory has no final record separator");
    }
    let records = &records[..records.len() - 1];
    if records.is_empty() {
        anyhow::bail!("Git worktree inventory has no main worktree record");
    }

    let mut paths = HashSet::with_capacity(records.len());
    let mut expected_registered = false;
    for (record_index, record) in records.iter().enumerate() {
        if record.is_empty() {
            anyhow::bail!("Git worktree inventory contains an empty record");
        }
        let fields = record.split('\0').collect::<Vec<_>>();
        if fields.iter().any(|field| field.is_empty()) {
            anyhow::bail!("Git worktree inventory contains an empty field");
        }
        let Some(path) = fields[0].strip_prefix("worktree ") else {
            anyhow::bail!(
                "Git worktree inventory record {} does not start with a worktree path",
                record_index + 1
            );
        };
        if path.is_empty() {
            anyhow::bail!("Git worktree inventory contains an empty worktree path");
        }
        if !paths.insert(path) {
            anyhow::bail!("Git worktree inventory repeats a worktree path");
        }
        expected_registered |= path == expected_path;

        let mut saw_head = false;
        let mut saw_branch = false;
        let mut saw_detached = false;
        let mut saw_bare = false;
        let mut saw_locked = false;
        let mut saw_prunable = false;
        for field in fields.iter().skip(1) {
            if let Some(head) = field.strip_prefix("HEAD ") {
                if saw_head || !valid_full_object_id(head) {
                    anyhow::bail!(
                        "Git worktree inventory record {} has an invalid HEAD field",
                        record_index + 1
                    );
                }
                saw_head = true;
            } else if let Some(branch) = field.strip_prefix("branch ") {
                if saw_branch
                    || !branch.starts_with("refs/heads/")
                    || branch.len() == "refs/heads/".len()
                    || branch.bytes().any(|byte| byte.is_ascii_whitespace())
                {
                    anyhow::bail!(
                        "Git worktree inventory record {} has an invalid branch field",
                        record_index + 1
                    );
                }
                saw_branch = true;
            } else if *field == "detached" {
                if saw_detached {
                    anyhow::bail!(
                        "Git worktree inventory record {} repeats detached state",
                        record_index + 1
                    );
                }
                saw_detached = true;
            } else if *field == "bare" {
                if saw_bare {
                    anyhow::bail!(
                        "Git worktree inventory record {} repeats bare state",
                        record_index + 1
                    );
                }
                saw_bare = true;
            } else if *field == "locked" || field.starts_with("locked ") {
                if saw_locked || field.strip_prefix("locked ").is_some_and(str::is_empty) {
                    anyhow::bail!(
                        "Git worktree inventory record {} has an invalid locked field",
                        record_index + 1
                    );
                }
                saw_locked = true;
            } else if *field == "prunable" || field.starts_with("prunable ") {
                if saw_prunable || field.strip_prefix("prunable ").is_some_and(str::is_empty) {
                    anyhow::bail!(
                        "Git worktree inventory record {} has an invalid prunable field",
                        record_index + 1
                    );
                }
                saw_prunable = true;
            } else {
                anyhow::bail!(
                    "Git worktree inventory record {} has an unknown field",
                    record_index + 1
                );
            }
        }
        if saw_bare {
            if saw_head || saw_branch || saw_detached {
                anyhow::bail!(
                    "Git worktree inventory record {} has conflicting bare state",
                    record_index + 1
                );
            }
        } else if !saw_head || saw_branch == saw_detached {
            anyhow::bail!(
                "Git worktree inventory record {} has invalid or incomplete HEAD state",
                record_index + 1
            );
        }
    }

    Ok(expected_registered)
}

fn valid_full_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Check whether the branch remains. A failed capture is an error because
/// callers must not remove the isolation record without proving absence.
async fn branch_still_present(
    runner: &mut impl CommandRunner,
    repo: &str,
    branch: &str,
) -> anyhow::Result<bool> {
    let output = runner
        .capture("git", &["-C", repo, "branch", "--list", branch], None)
        .await?;
    Ok(!output.trim().is_empty())
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
