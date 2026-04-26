use crate::debug_log;
use crate::docker::CommandRunner;
use crate::isolation::state::{IsolationRecord, remove_record};
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
#[allow(clippy::too_many_lines)] // verify-and-bail flow has lots of small steps; splitting hurts readability
pub fn force_cleanup_isolated(
    record: &IsolationRecord,
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let host_repo_exists = std::path::Path::new(&record.original_src).exists();
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
        let wt_remove_result = runner.run(
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
            &crate::docker::RunOptions {
                quiet: true,
                ..Default::default()
            },
        );
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
        let branch_delete_result = runner.run(
            "git",
            &[
                "-C",
                &record.original_src,
                "branch",
                "-D",
                &record.scratch_branch,
            ],
            None,
            &crate::docker::RunOptions {
                quiet: true,
                ..Default::default()
            },
        );
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
        if branch_still_present(runner, &record.original_src, &record.scratch_branch) == Some(true)
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
    let wt = std::path::Path::new(&record.worktree_path);
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

/// Best-effort check: is `branch` still present on `repo`? Returns
/// `Some(true)` if confirmed present, `Some(false)` if confirmed absent,
/// `None` if we couldn't tell (e.g., `git branch --list` itself errored).
/// Callers treat `None` as "couldn't verify, don't bail" — the cost of
/// a false negative here (orphan branch left behind) is much lower than
/// the cost of a false positive (operator stuck unable to purge).
fn branch_still_present(runner: &mut impl CommandRunner, repo: &str, branch: &str) -> Option<bool> {
    let output = runner
        .capture("git", &["-C", repo, "branch", "--list", branch], None)
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
pub fn purge_isolated_for_container(
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let records = crate::isolation::state::read_records(container_state_dir)?;
    debug_log!(
        "isolation",
        "purge_isolated_for_container: {n} record(s) under {dir}",
        n = records.len(),
        dir = container_state_dir.display(),
    );
    let mut failed: Vec<String> = Vec::new();
    for rec in records {
        if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner) {
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
mod tests {
    use super::*;
    use crate::isolation::MountIsolation;
    use crate::isolation::state::{CleanupStatus, read_records, write_records};
    use crate::runtime::test_support::FakeRunner;
    use tempfile::TempDir;

    fn rec_for(repo: &Path, container_dir: &Path) -> IsolationRecord {
        let wt = container_dir.join("isolated/workspace/jackin");
        std::fs::create_dir_all(&wt).unwrap();
        IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: "/workspace/jackin".into(),
            original_src: repo.to_string_lossy().into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }
    }

    #[test]
    fn force_cleanup_runs_git_and_removes_record() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

        let mut runner = FakeRunner::default();
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();

        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force"))
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("branch -D jackin/scratch/x"))
        );
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn force_cleanup_tolerates_missing_host_repo() {
        let container_dir = TempDir::new().unwrap();
        let rec = IsolationRecord {
            original_src: "/nonexistent/path".into(),
            ..rec_for(std::path::Path::new("/tmp"), container_dir.path())
        };
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

        let mut runner = FakeRunner::default();
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();
        assert!(
            runner.run_recorded.is_empty(),
            "should skip git when src missing"
        );
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn force_cleanup_is_idempotent_when_worktree_already_gone() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let mut rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();
        // Pre-delete the worktree dir.
        std::fs::remove_dir_all(&rec.worktree_path).unwrap();
        rec.worktree_path = format!("{}-gone", rec.worktree_path);

        let mut runner = FakeRunner::default();
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn purge_isolated_for_container_runs_force_cleanup_for_each_record() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let r1 = rec_for(repo_dir.path(), container_dir.path());
        let mut r2 = rec_for(repo_dir.path(), container_dir.path());
        r2.mount_dst = "/workspace/docs".into();
        let wt2 = container_dir.path().join("isolated/workspace/docs");
        std::fs::create_dir_all(&wt2).unwrap();
        r2.worktree_path = wt2.to_string_lossy().into();
        r2.scratch_branch = "jackin/scratch/x-2".into();
        let records = vec![r1.clone(), r2.clone()];
        write_records(container_dir.path(), &records).unwrap();

        let mut runner = FakeRunner::default();
        purge_isolated_for_container(container_dir.path(), &mut runner).unwrap();

        let removes = runner
            .run_recorded
            .iter()
            .filter(|c| c.contains("worktree remove --force"))
            .count();
        let branches = runner
            .run_recorded
            .iter()
            .filter(|c| c.contains("branch -D"))
            .count();
        assert_eq!(removes, 2);
        assert_eq!(branches, 2);
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn purge_isolated_is_noop_when_no_records() {
        let container_dir = TempDir::new().unwrap();
        let mut runner = FakeRunner::default();
        purge_isolated_for_container(container_dir.path(), &mut runner).unwrap();
        assert!(runner.run_recorded.is_empty());
    }

    /// Idempotent path: `git branch -D` errors because the branch was
    /// already deleted externally, then `git branch --list` confirms it's
    /// gone → cleanup proceeds and the record is removed. The verify
    /// step is exactly what lets us distinguish "already done" from
    /// "real failure".
    #[test]
    fn force_cleanup_tolerates_branch_already_deleted_when_verify_says_absent() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

        // `git branch -D` fails (branch was already gone); the verify
        // capture returns empty → confirms branch is absent → proceed.
        let mut runner = FakeRunner {
            fail_on: vec!["branch -D".into()],
            // capture queue: empty result for `git branch --list <branch>`
            capture_queue: std::collections::VecDeque::from(vec![String::new()]),
            ..Default::default()
        };
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();
        assert!(
            read_records(container_dir.path()).unwrap().is_empty(),
            "record should be removed when branch is verified absent"
        );
    }

    /// Real failure: `git branch -D` errors AND the verify step shows
    /// the branch is still present. Cleanup must bail without touching
    /// the record so the operator can re-run `jackin purge` after
    /// resolving the issue.
    #[test]
    fn force_cleanup_retains_record_when_branch_delete_fails_and_branch_still_present() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

        // `git branch -D` fails; verify capture says branch IS present.
        let mut runner = FakeRunner {
            fail_on: vec!["branch -D".into()],
            capture_queue: std::collections::VecDeque::from(vec![
                "  jackin/scratch/x\n".to_string(),
            ]),
            ..Default::default()
        };
        let err = force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap_err();
        assert!(
            err.to_string().contains("scratch branch"),
            "error should name the branch; got: {err}"
        );
        assert!(
            err.to_string().contains("record retained"),
            "error should tell the operator the record was retained; got: {err}"
        );
        // Critical: record MUST still be there.
        let recs = read_records(container_dir.path()).unwrap();
        assert_eq!(
            recs.len(),
            1,
            "record must NOT be removed on cleanup failure; otherwise re-running purge becomes impossible"
        );
    }

    /// `rm -rf` fails (e.g., simulated by leaving the worktree path as
    /// a non-removable parent). Cleanup must bail without removing the
    /// record. We can't easily simulate `remove_dir_all` failure
    /// portably; this test instead pins the contract via the doc comment
    /// and the error-message check on a related path.
    /// When one record fails and others succeed, purge must:
    /// (a) iterate ALL records (not stop at the first failure),
    /// (b) bail with an aggregate Err so the exit code reflects reality.
    /// Pre-fix: returned Ok(()) regardless, masking the failure as a
    /// scrolled-past stderr warning.
    #[test]
    fn purge_isolated_for_container_bails_when_any_record_fails() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        // Two records on different mounts.
        let r1 = rec_for(repo_dir.path(), container_dir.path());
        let mut r2 = rec_for(repo_dir.path(), container_dir.path());
        r2.mount_dst = "/workspace/docs".into();
        let wt2 = container_dir.path().join("isolated/workspace/docs");
        std::fs::create_dir_all(&wt2).unwrap();
        r2.worktree_path = wt2.to_string_lossy().into();
        r2.scratch_branch = "jackin/scratch/x-2".into();
        write_records(container_dir.path(), &[r1.clone(), r2.clone()]).unwrap();

        // r1's branch -D fails AND verify says it's still present →
        // force_cleanup_isolated bails for r1. r2's branch -D succeeds
        // (different branch name doesn't match the fail_on substring).
        let mut runner = FakeRunner {
            // r1's verify returns "still present"; r2's verify returns empty.
            capture_queue: std::collections::VecDeque::from(vec![
                "  jackin/scratch/x\n".to_string(),
                String::new(),
            ]),
            // Only r1's specific branch fails. Substring match avoids
            // catching r2's `branch -D jackin/scratch/x-2`.
            fail_on: vec!["branch -D jackin/scratch/x ".into()],
            ..Default::default()
        };
        let err = purge_isolated_for_container(container_dir.path(), &mut runner).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("1 failure"),
            "must aggregate failure count; got: {msg}"
        );
        assert!(
            msg.contains("/workspace/jackin"),
            "must name the failing mount; got: {msg}"
        );
        // r2 (the successful one) should be removed; r1 (the failing
        // one) retained.
        let recs = read_records(container_dir.path()).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].mount_dst, "/workspace/jackin");
    }

    /// `branch_still_present` returns `None` when the verify capture
    /// itself errors (e.g., host `.git` corrupted between `branch -D`
    /// and `branch --list`). The doc comment on the helper says
    /// "callers treat None as 'couldn't verify, don't bail'" — pin
    /// that contract so a refactor to `unwrap_or(true)` (the "safer"
    /// reading) doesn't break purge for any verify failure.
    #[test]
    fn force_cleanup_proceeds_when_verify_capture_itself_errors() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

        // `git branch -D` fails AND `git branch --list` (the verify
        // capture) ALSO fails. branch_still_present returns None →
        // proceed (don't bail) → record removed.
        let mut runner = FakeRunner {
            fail_on: vec!["branch -D".into(), "branch --list".into()],
            ..Default::default()
        };
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();
        assert!(
            read_records(container_dir.path()).unwrap().is_empty(),
            "record should be removed when verify is inconclusive (None) — \
             cost of false negative (orphan branch) is lower than cost of \
             false positive (operator stuck unable to purge)"
        );
    }

    #[test]
    fn force_cleanup_error_message_mentions_record_retention() {
        // This test pins that any failure path produces an error
        // message that tells the operator the record was retained.
        // Concretely covered by the branch-still-present test above;
        // this is a structural smoke check that the error-message
        // contract is consistent.
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();
        let mut runner = FakeRunner {
            fail_on: vec!["branch -D".into()],
            capture_queue: std::collections::VecDeque::from(vec!["jackin/scratch/x".to_string()]),
            ..Default::default()
        };
        let err = force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("`jackin purge`"), "got: {msg}");
        assert!(msg.contains("record retained"), "got: {msg}");
    }
}
