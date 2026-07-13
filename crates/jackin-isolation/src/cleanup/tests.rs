//! Tests for `cleanup`.
use super::*;
use crate::MountIsolation;
use crate::state::{CleanupStatus, read_records, write_records};
use jackin_test_support::FakeRunner;
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

#[tokio::test]
async fn force_cleanup_runs_git_and_removes_record() {
    let repo_dir = TempDir::new().unwrap();
    let container_dir = TempDir::new().unwrap();
    let rec = rec_for(repo_dir.path(), container_dir.path());
    write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

    let mut runner = FakeRunner::default();
    force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap();

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

#[tokio::test]
async fn force_cleanup_clone_removes_directory_without_host_git_ops() {
    let repo_dir = TempDir::new().unwrap();
    let container_dir = TempDir::new().unwrap();
    let clone_dir = container_dir
        .path()
        .join("git/clone/repo/workspace/jackin/jackin-x");
    std::fs::create_dir_all(clone_dir.join(".git")).unwrap();
    let rec = IsolationRecord {
        isolation: MountIsolation::Clone,
        worktree_path: clone_dir.to_string_lossy().into(),
        ..rec_for(repo_dir.path(), container_dir.path())
    };
    write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

    let mut runner = FakeRunner::default();
    force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap();

    assert!(runner.run_recorded.is_empty());
    assert!(!clone_dir.exists());
    assert!(read_records(container_dir.path()).unwrap().is_empty());
}

#[tokio::test]
async fn force_cleanup_tolerates_missing_host_repo() {
    let container_dir = TempDir::new().unwrap();
    let rec = IsolationRecord {
        original_src: "/nonexistent/path".into(),
        ..rec_for(Path::new("/tmp"), container_dir.path())
    };
    write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

    let mut runner = FakeRunner::default();
    force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap();
    assert!(
        runner.run_recorded.is_empty(),
        "should skip git when src missing"
    );
    assert!(read_records(container_dir.path()).unwrap().is_empty());
}

#[tokio::test]
async fn force_cleanup_is_idempotent_when_worktree_already_gone() {
    let repo_dir = TempDir::new().unwrap();
    let container_dir = TempDir::new().unwrap();
    let mut rec = rec_for(repo_dir.path(), container_dir.path());
    write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();
    // Pre-delete the worktree dir.
    std::fs::remove_dir_all(&rec.worktree_path).unwrap();
    rec.worktree_path = format!("{}-gone", rec.worktree_path);

    let mut runner = FakeRunner::default();
    force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap();
    assert!(read_records(container_dir.path()).unwrap().is_empty());
}

#[tokio::test]
async fn purge_isolated_for_container_runs_force_cleanup_for_each_record() {
    let repo_dir = TempDir::new().unwrap();
    let container_dir = TempDir::new().unwrap();
    let r1 = rec_for(repo_dir.path(), container_dir.path());
    let mut r2 = rec_for(repo_dir.path(), container_dir.path());
    r2.mount_dst = "/workspace/docs".into();
    let wt2 = container_dir.path().join("isolated/workspace/docs");
    std::fs::create_dir_all(&wt2).unwrap();
    r2.worktree_path = wt2.to_string_lossy().into();
    r2.scratch_branch = "jackin/scratch/x-2".into();
    let records = vec![r1, r2];
    write_records(container_dir.path(), &records).unwrap();

    let mut runner = FakeRunner::default();
    purge_isolated_for_container(container_dir.path(), &mut runner)
        .await
        .unwrap();

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

#[tokio::test]
async fn purge_isolated_is_noop_when_no_records() {
    let container_dir = TempDir::new().unwrap();
    let mut runner = FakeRunner::default();
    purge_isolated_for_container(container_dir.path(), &mut runner)
        .await
        .unwrap();
    assert!(runner.run_recorded.is_empty());
}

/// Idempotent path: `git branch -D` errors because the branch was
/// already deleted externally, then `git branch --list` confirms it's
/// gone → cleanup proceeds and the record is removed. The verify
/// step is exactly what lets us distinguish "already done" from
/// "real failure".
#[tokio::test]
async fn force_cleanup_tolerates_branch_already_deleted_when_verify_says_absent() {
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
    force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap();
    assert!(
        read_records(container_dir.path()).unwrap().is_empty(),
        "record should be removed when branch is verified absent"
    );
}

/// Real failure: `git branch -D` errors AND the verify step shows
/// the branch is still present. Cleanup must bail without touching
/// the record so the operator can re-run `jackin purge` after
/// resolving the issue.
#[tokio::test]
async fn force_cleanup_retains_record_when_branch_delete_fails_and_branch_still_present() {
    let repo_dir = TempDir::new().unwrap();
    let container_dir = TempDir::new().unwrap();
    let rec = rec_for(repo_dir.path(), container_dir.path());
    write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

    // `git branch -D` fails; verify capture says branch IS present.
    let mut runner = FakeRunner {
        fail_on: vec!["branch -D".into()],
        capture_queue: std::collections::VecDeque::from(vec!["  jackin/scratch/x\n".to_owned()]),
        ..Default::default()
    };
    let err = force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap_err();
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
#[tokio::test]
async fn purge_isolated_for_container_bails_when_any_record_fails() {
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
    write_records(container_dir.path(), &[r1, r2]).unwrap();

    // r1's branch -D fails AND verify says it's still present →
    // force_cleanup_isolated bails for r1. r2's branch -D succeeds
    // (different branch name doesn't match the fail_on substring).
    let mut runner = FakeRunner {
        // r1's verify returns "still present"; r2's verify returns empty.
        capture_queue: std::collections::VecDeque::from(vec![
            "  jackin/scratch/x\n".to_owned(),
            String::new(),
        ]),
        // Only r1's specific branch fails. Substring match avoids
        // catching r2's `branch -D jackin/scratch/x-2`.
        fail_on: vec!["branch -D jackin/scratch/x ".into()],
        ..Default::default()
    };
    let err = purge_isolated_for_container(container_dir.path(), &mut runner)
        .await
        .unwrap_err();
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
#[tokio::test]
async fn force_cleanup_proceeds_when_verify_capture_itself_errors() {
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
    force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap();
    assert!(
        read_records(container_dir.path()).unwrap().is_empty(),
        "record should be removed when verify is inconclusive (None) — \
             cost of false negative (orphan branch) is lower than cost of \
             false positive (operator stuck unable to purge)"
    );
}

#[tokio::test]
async fn force_cleanup_error_message_mentions_record_retention() {
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
        capture_queue: std::collections::VecDeque::from(vec!["jackin/scratch/x".to_owned()]),
        ..Default::default()
    };
    let err = force_cleanup_isolated(&rec, container_dir.path(), &mut runner)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("`jackin purge`"), "got: {msg}");
    assert!(msg.contains("record retained"), "got: {msg}");
}
