// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `finalize`.
use super::*;
use jackin_config::DirtyExitPolicy;
use tempfile::TempDir;

struct NoPrompt;
impl FinalizerPrompt for NoPrompt {
    fn ask_exit_dialog(
        &mut self,
        _c: &str,
        _records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        panic!("prompt should not be called in this test");
    }
}

#[test]
fn rich_exit_dialog_keeps_all_when_rich_dialog_is_unavailable() {
    use crate::MountIsolation;
    use crate::state::{CleanupStatus, IsolationRecord};
    let mut prompt = RichCleanupPrompt;
    let record = IsolationRecord {
        workspace: "test".into(),
        mount_dst: "/workspace/test".into(),
        original_src: "/tmp/repo".into(),
        isolation: MountIsolation::Worktree,
        worktree_path: "/tmp/jackin-preserved-worktree".into(),
        scratch_branch: "jackin/scratch/test".into(),
        base_commit: "abc".into(),
        selector_key: "test".into(),
        container_name: "jk-test".into(),
        cleanup_status: CleanupStatus::Active,
    };
    let choice = prompt
        .ask_exit_dialog("jk-test", &[(record, PreservedReason::Dirty)])
        .unwrap();
    assert_eq!(
        choice,
        ExitDialogChoice::KeepAll,
        "without a rich dialog, exit must keep all instead of falling back to a numbered CLI prompt"
    );
}

use jackin_runtime::runtime::test_support::FakeRunner;

#[tokio::test]
async fn still_running_with_zero_sessions_cleans() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from(["Sessions: 0\n".to_owned()])),
        ..Default::default()
    };
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::still_running(),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut r,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
}

#[tokio::test]
async fn still_running_with_unparseable_status_preserves_records() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([String::new()])),
        ..Default::default()
    };
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::still_running(),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut r,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
}

#[tokio::test]
async fn still_running_with_sessions_preserves() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            "Sessions: 1\n  [3] work (claude) state=working active=true\n".to_owned(),
        ])),
        ..Default::default()
    };
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::still_running(),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut r,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
}

#[tokio::test]
async fn stopped_non_zero_preserves_records() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(137),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut r,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
}

#[tokio::test]
async fn oom_killed_preserves_records() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::oom_killed(),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut r,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
}

use crate::MountIsolation;
use crate::state::write_records;
use std::collections::VecDeque;

fn fake_with_outputs(outputs: &[&str]) -> FakeRunner {
    FakeRunner {
        capture_queue: VecDeque::from(outputs.iter().map(ToString::to_string).collect::<Vec<_>>()),
        ..FakeRunner::default()
    }
}

fn rec(container_dir: &Path) -> IsolationRecord {
    let wt = container_dir.join("isolated/workspace/jackin");
    std::fs::create_dir_all(&wt).unwrap();
    IsolationRecord {
        workspace: "jackin".into(),
        mount_dst: "/workspace/jackin".into(),
        original_src: container_dir.join("repo").to_string_lossy().into(),
        isolation: MountIsolation::Worktree,
        worktree_path: wt.to_string_lossy().into(),
        scratch_branch: "jackin/scratch/x".into(),
        base_commit: "abc".into(),
        selector_key: "x".into(),
        container_name: "jackin-x".into(),
        cleanup_status: CleanupStatus::Active,
    }
}

/// Format one for-each-ref row exactly the way the production
/// query renders it (tab-separated columns).
fn ferow(name: &str, tip: &str, upstream: &str, track: &str) -> String {
    format!("{name}\t{tip}\t{upstream}\t{track}")
}

#[tokio::test]
async fn clean_worktree_with_head_equal_base_deletes_record() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();

    // Capture queue:
    //   status --porcelain           (clean)
    //   for-each-ref refs/heads/     (single scratch branch at base_commit)
    //   symbolic-ref HEAD            (HEAD on scratch branch → attached)
    let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
    let mut runner = fake_with_outputs(&["", &branches, "refs/heads/jackin/scratch/x"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree remove --force"))
    );
    assert!(runner.run_recorded.iter().any(|c| c.contains("branch -D")));
}

#[tokio::test]
async fn clean_worktree_with_pushed_commits_deletes_record() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Capture queue:
    //   status --porcelain (clean)
    //   for-each-ref -> single branch ahead of base with reachable upstream
    //   rev-list <upstream>..<branch> -> "" (all reachable)
    //   symbolic-ref HEAD            (HEAD on scratch branch → attached)
    let branches = format!(
        "{}\n",
        ferow("jackin/scratch/x", "newhead", "origin/jackin/scratch/x", "",)
    );
    let mut runner = fake_with_outputs(&["", &branches, "", "refs/heads/jackin/scratch/x"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
}

#[tokio::test]
async fn clean_worktree_with_unpushed_commits_preserves() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Capture queue:
    //   status --porcelain (clean)
    //   for-each-ref -> single branch ahead of base with upstream set
    //   rev-list <upstream>..<branch> -> "deadbeef" (one local commit not on upstream)
    let branches = format!(
        "{}\n",
        ferow(
            "jackin/scratch/x",
            "newhead",
            "origin/jackin/scratch/x",
            "[ahead 1]",
        )
    );
    let mut runner = fake_with_outputs(&["", &branches, "deadbeef\n"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

#[tokio::test]
async fn clean_worktree_no_upstream_preserves_when_head_diverged() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Capture queue:
    //   status --porcelain (clean)
    //   for-each-ref -> single branch ahead of base with no upstream
    let branches = format!("{}\n", ferow("jackin/scratch/x", "newhead", "", ""));
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

struct ScriptedPrompt(VecDeque<ExitDialogChoice>);
impl FinalizerPrompt for ScriptedPrompt {
    fn ask_exit_dialog(
        &mut self,
        _c: &str,
        _records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        Ok(self.0.pop_front().expect("scripted prompt exhausted"))
    }
}

/// Capture-and-assert version of `ScriptedPrompt`: records the reasons
/// passed per record so tests can pin that the correct assessment wording
/// reaches the dialog (D23: all records shown in one call).
struct RecordingPrompt {
    answer: ExitDialogChoice,
    seen: Vec<PreservedReason>,
}

impl RecordingPrompt {
    fn new(answer: ExitDialogChoice) -> Self {
        Self {
            answer,
            seen: Vec::new(),
        }
    }
}

impl FinalizerPrompt for RecordingPrompt {
    fn ask_exit_dialog(
        &mut self,
        _c: &str,
        records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        for (_, r) in records {
            self.seen.push(*r);
        }
        Ok(self.answer)
    }
}

#[tokio::test]
async fn dirty_worktree_interactive_preserve_choice_keeps_state() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::KeepAll]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
}

#[tokio::test]
async fn dirty_worktree_interactive_force_delete_runs_cleanup() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::DiscardAll]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree remove --force"))
    );
}

#[tokio::test]
async fn dirty_worktree_interactive_return_to_agent_signals_caller() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::ReturnToRole]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::ReturnToAgent);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
}

#[tokio::test]
async fn dirty_worktree_non_interactive_prints_warning_and_preserves() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
}

/// Build a runner whose git captures will fail at a specific stage.
/// `fail_pattern` is matched as substring against the recorded command.
/// Anything before the failing capture comes from `outputs`.
fn fake_failing_capture(outputs: &[&str], fail_pattern: &str) -> FakeRunner {
    FakeRunner {
        capture_queue: VecDeque::from(outputs.iter().map(ToString::to_string).collect::<Vec<_>>()),
        fail_on: vec![fail_pattern.into()],
        ..FakeRunner::default()
    }
}

// The block of tests below pins the safety contract from the docstring
// on `assess_cleanup`: any git capture failure must route to
// `PreservedUnpushed` (not `SafeToDelete`) so a transient git error
// never garbage-collects unpushed scratch-branch commits.

#[tokio::test]
async fn assess_cleanup_status_capture_failure_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status --porcelain errors → must NOT be treated as clean tree.
    let mut runner = fake_failing_capture(&[], "status --porcelain");
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    // Critically: no git worktree remove / branch -D should have run.
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree remove --force")),
        "must not delete worktree when status capture failed; recorded={:?}",
        runner.run_recorded,
    );
}

#[tokio::test]
async fn assess_cleanup_for_each_ref_failure_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status clean, then for-each-ref refs/heads/ errors.
    let mut runner = fake_failing_capture(&[""], "for-each-ref");
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree remove --force"))
    );
}

#[tokio::test]
async fn assess_cleanup_rev_list_failure_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status clean, for-each-ref returns one branch ahead with
    // upstream still configured (not gone), then rev-list fails.
    // The fail-closed Err arm must route to PreservedUnpushed, not
    // silently treat the failure as "no commits ahead".
    let branches = format!(
        "{}\n",
        ferow(
            "jackin/scratch/x",
            "newhead",
            "origin/jackin/scratch/x",
            "[ahead 1]",
        )
    );
    let mut runner = fake_failing_capture(&["", &branches], "rev-list");
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree remove --force")),
        "rev-list failure must not auto-delete; recorded={:?}",
        runner.run_recorded,
    );
}

fn rec_at(container_dir: &Path, mount_dst: &str, scratch_branch: &str) -> IsolationRecord {
    let rel = mount_dst.trim_matches('/');
    let wt = container_dir.join(format!("isolated/{rel}"));
    std::fs::create_dir_all(&wt).unwrap();
    IsolationRecord {
        workspace: "ws".into(),
        mount_dst: mount_dst.into(),
        original_src: container_dir.join("repo").to_string_lossy().into(),
        isolation: MountIsolation::Worktree,
        worktree_path: wt.to_string_lossy().into(),
        scratch_branch: scratch_branch.into(),
        base_commit: "abc".into(),
        selector_key: "x".into(),
        container_name: "jackin-x".into(),
        cleanup_status: CleanupStatus::Active,
    }
}

/// Multi-mount workspace with two preserved records, operator picks
/// "force delete" on both. Both worktrees must be cleaned and the
/// caller signaled `Cleaned` so the container teardown proceeds.
#[tokio::test]
async fn multi_mount_force_delete_on_each_cleans_all_records() {
    let dir = TempDir::new().unwrap();
    let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
    let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
    std::fs::create_dir_all(&r1.original_src).unwrap();
    write_records(dir.path(), &[r1, r2]).unwrap();
    // Both records assess to PreservedDirty (status returns dirty for each).
    let mut runner = fake_with_outputs(&[" M file\n", " M file\n"]);
    // Operator chooses option 2 (force delete) for both.
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::DiscardAll]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(
        read_records(dir.path()).unwrap().is_empty(),
        "both records should be removed after force-delete on both",
    );
    let removes = runner
        .run_recorded
        .iter()
        .filter(|c| c.contains("worktree remove --force"))
        .count();
    assert_eq!(
        removes, 2,
        "must run worktree-remove for BOTH preserved mounts; recorded={:?}",
        runner.run_recorded
    );
}

/// Multi-mount workspace with two preserved records; operator picks "keep all"
/// via the D23 one-screen dialog. Both records remain on disk and the caller
/// gets `Preserved` so the container is not torn down.
#[tokio::test]
async fn multi_mount_keep_all_signals_preserved() {
    let dir = TempDir::new().unwrap();
    let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
    let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
    std::fs::create_dir_all(&r1.original_src).unwrap();
    write_records(dir.path(), &[r1, r2]).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n", " M file\n"]);
    // D23: one dialog for all records; operator picks keep all.
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::KeepAll]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(
        dec,
        FinalizeDecision::Preserved,
        "KeepAll must signal Preserved so the container is not torn down",
    );
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs.len(), 2, "both records must remain preserved");
}

/// `ReturnToAgent` on the D23 one-screen dialog returns to the role without
/// deleting any records. All three records remain and the caller gets
/// `ReturnToAgent` so the container is restarted.
#[tokio::test]
async fn multi_mount_return_to_agent_signals_return_to_agent() {
    let dir = TempDir::new().unwrap();
    let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
    let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
    let r3 = rec_at(dir.path(), "/workspace/c", "jackin/scratch/x-c");
    std::fs::create_dir_all(&r1.original_src).unwrap();
    write_records(dir.path(), &[r1, r2, r3]).unwrap();
    let mut runner = fake_with_outputs(&[" M f1\n", " M f2\n", " M f3\n"]);
    // D23: one dialog for all records; operator returns to role.
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::ReturnToRole]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::ReturnToAgent);
    // No worktrees removed — all three records still on disk.
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs.len(), 3, "ReturnToAgent must not delete any records");
    let removes = runner
        .run_recorded
        .iter()
        .filter(|c| c.contains("worktree remove --force"))
        .count();
    assert_eq!(
        removes, 0,
        "ReturnToAgent must run no worktree-remove; recorded={:?}",
        runner.run_recorded
    );
}

/// `force_cleanup_isolated` failing partway through the prompt loop
/// must NOT propagate as Err. The caller would see a raw cleanup
/// error, the container would be left running without a Preserved
/// signal, and any subsequent records would never be prompted.
/// Instead the failure is logged and the loop continues.
#[tokio::test]
async fn multi_mount_cleanup_failure_in_loop_does_not_abort() {
    let dir = TempDir::new().unwrap();
    let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
    let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
    std::fs::create_dir_all(&r1.original_src).unwrap();
    write_records(dir.path(), &[r1, r2]).unwrap();
    // Both records assess to PreservedDirty (status returns dirty
    // for each), then force_cleanup_isolated runs git commands.
    // We simulate the first mount's `git branch -D` failing AND
    // the verify capture (`git branch --list`) confirming the
    // branch is still present → force_cleanup_isolated bails.
    // Pre-fix: that bail would propagate via `?` and the second
    // record would never be prompted.
    let mut runner = FakeRunner {
        // Capture queue: status for r1, status for r2, then verify
        // capture for r1 (says branch IS present — triggers bail),
        // then verify capture for r2 (says branch absent — proceed).
        capture_queue: VecDeque::from([
            " M f1\n".to_owned(),
            " M f2\n".to_owned(),
            "  jackin/scratch/x-a\n".to_owned(),
            String::new(),
        ]),
        // git branch -D for r1's branch fails; r2's branch -D succeeds.
        fail_on: vec!["branch -D jackin/scratch/x-a".into()],
        ..FakeRunner::default()
    };
    // Operator force-deletes both.
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::DiscardAll]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .expect("loop must NOT propagate the cleanup Err — caller would see a raw error");
    assert_eq!(
        dec,
        FinalizeDecision::Preserved,
        "first record's failed cleanup must surface as Preserved (record retained); \
             second record was successfully force-deleted",
    );
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(
        recs.len(),
        1,
        "first record retained (cleanup bailed), second removed (force-deleted ok)"
    );
    assert_eq!(recs[0].mount_dst, "/workspace/a");
}

/// Non-interactive multi-mount: every preserved record's path is
/// printed to stderr so the operator sees all of them, not just the
/// first.
#[tokio::test]
async fn multi_mount_non_interactive_marks_all_preserved() {
    let dir = TempDir::new().unwrap();
    let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
    let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
    std::fs::create_dir_all(&r1.original_src).unwrap();
    write_records(dir.path(), &[r1, r2]).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n", " M file\n"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs.len(), 2);
    assert!(
        recs.iter()
            .all(|r| r.cleanup_status == CleanupStatus::PreservedDirty),
        "every record must be marked preserved, not just the first",
    );
}

#[tokio::test]
async fn assess_cleanup_empty_for_each_ref_preserves_unpushed() {
    // Defense in depth: a worktree that reports zero local branches
    // is pathological — even a freshly materialized worktree carries
    // the scratch branch. Refuse to delete what we can't account for.
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status clean, then for-each-ref returns empty (no branches).
    let mut runner = fake_with_outputs(&["", ""]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

// ---------------------------------------------------------------
// Per-branch policy table (Piece 1 of the worktree-cleanup
// assessment fix). Each test pins one row of the table, with the
// worktree state shape that produces the captured-state bug from
// worktree-cleanup-assessment.mdx.
// ---------------------------------------------------------------

/// Renamed-branch happy path. Scratch branch parked at `base_commit`;
/// role's renamed `feature/x` branch is ahead of base with a
/// reachable upstream and rev-list returns empty (all commits
/// pushed). Pre-fix this returned `PreservedUnpushed` because the
/// upstream check was hardcoded against the abandoned scratch
/// branch (which has no upstream by construction).
#[tokio::test]
async fn renamed_branch_pushed_clean_is_safe_to_delete() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status clean,
    // for-each-ref enumerates two branches:
    //   - scratch at base_commit (no upstream)
    //   - feature/x ahead, upstream set, no [gone]
    // rev-list <upstream>..feature/x → empty (everything pushed)
    // symbolic-ref HEAD             (HEAD on feature/x → attached)
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/x", "newhead", "origin/feature/x", ""),
    ]
    .join("\n")
        + "\n";
    let mut runner = fake_with_outputs(&["", &branches, "", "refs/heads/feature/x"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
}

/// Squash-merged-and-pruned branch. Scratch parked at base; the
/// role's `feature/x` branch is ahead with upstream set, but the
/// upstream-tracking column shows `[gone]` because the remote
/// branch was deleted after the PR merge and pruned locally. The
/// `[gone]` heuristic must mark this Safe; pre-fix the rev-list
/// would have errored on the missing upstream and the Err arm
/// would have routed to `PreservedUnpushed`.
#[tokio::test]
async fn squash_merged_pruned_branch_is_safe_to_delete() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/x", "newhead", "origin/feature/x", "[gone]"),
    ]
    .join("\n")
        + "\n";
    // No rev-list call expected — `[gone]` short-circuits to Safe.
    // symbolic-ref HEAD  (HEAD on feature/x → attached)
    let mut runner = fake_with_outputs(&["", &branches, "refs/heads/feature/x"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
    assert!(
        !runner.recorded.iter().any(|c| c.contains("rev-list")),
        "[gone] short-circuit must not invoke rev-list; recorded={:?}",
        runner.recorded,
    );
}

/// Renamed branch ahead of base with no upstream — genuine local
/// work; preserve. Pre-fix this also returned `PreservedUnpushed`
/// (correct outcome) but only by accident of the wrong-branch check.
#[tokio::test]
async fn renamed_branch_no_upstream_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/x", "newhead", "", ""),
    ]
    .join("\n")
        + "\n";
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

/// Renamed branch ahead with upstream set, rev-list returns commits
/// → real unpushed work, preserve.
#[tokio::test]
async fn renamed_branch_with_unpushed_commits_preserves() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/x", "newhead", "origin/feature/x", "[ahead 2]"),
    ]
    .join("\n")
        + "\n";
    let mut runner = fake_with_outputs(&["", &branches, "deadbeef\ncafef00d\n"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

/// Multiple non-trivial branches, all safe by different paths
/// (one merged-and-pruned, one pushed-clean). All-Safe → cleanup.
#[tokio::test]
async fn multiple_branches_all_safe_deletes_record() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/a", "aaaa", "origin/feature/a", "[gone]"),
        ferow("feature/b", "bbbb", "origin/feature/b", ""),
    ]
    .join("\n")
        + "\n";
    // status clean, branch enumeration, rev-list for feature/b only
    // (feature/a short-circuits via [gone], scratch via tip==base),
    // then symbolic-ref HEAD (HEAD on feature/b → attached).
    let mut runner = fake_with_outputs(&["", &branches, "", "refs/heads/feature/b"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
    let revlist_calls = runner
        .recorded
        .iter()
        .filter(|c| c.contains("rev-list"))
        .count();
    assert_eq!(
        revlist_calls, 1,
        "[gone] and tip==base must short-circuit; only feature/b should hit rev-list. recorded={:?}",
        runner.recorded,
    );
}

/// Multiple branches, one with real unpushed work → preserve.
#[tokio::test]
async fn multiple_branches_one_unsafe_preserves() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/a", "aaaa", "origin/feature/a", ""),
        ferow("feature/b", "bbbb", "", ""), // ahead, no upstream → unsafe
    ]
    .join("\n")
        + "\n";
    // for-each-ref, then rev-list for feature/a (empty == pushed),
    // then enumeration hits feature/b and short-circuits to
    // PreservedUnpushed without another rev-list.
    let mut runner = fake_with_outputs(&["", &branches, ""]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

/// Prompt-wording variant: a `PreservedUnpushed` assessment must
/// reach the prompt with reason=Unpushed (not Dirty). Pre-fix,
/// `ask_unsafe_cleanup` had no reason argument so the wording was
/// hardcoded to "uncommitted changes" for both paths.
#[tokio::test]
async fn unpushed_branch_prompts_with_unpushed_reason() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
    // status clean → for-each-ref → ahead+no-upstream → preserve
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = RecordingPrompt::new(ExitDialogChoice::KeepAll); // operator picks "preserve"
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    assert_eq!(p.seen, vec![PreservedReason::Unpushed]);
}

/// Counterpart: a dirty worktree must reach the prompt with
/// reason=Dirty so the operator sees "uncommitted changes".
#[tokio::test]
async fn dirty_worktree_prompts_with_dirty_reason() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut p = RecordingPrompt::new(ExitDialogChoice::KeepAll);
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    assert_eq!(p.seen, vec![PreservedReason::Dirty]);
}

// ---------------------------------------------------------------
// Malformed for-each-ref row — fail-closed guard (line 377).
// These tests replace the old assess_cleanup_empty_head_*
// test (which exercised the removed rev-parse HEAD path). The
// equivalent invariant is now: an empty `name` or empty `tip`
// in a for-each-ref row must never match base_commit and must
// always route to PreservedUnpushed.
// ---------------------------------------------------------------

#[tokio::test]
async fn assess_cleanup_malformed_row_empty_name_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Empty name column — malformed row, fail closed.
    let branches = format!("{}\n", ferow("", "newhead", "", ""));
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

#[tokio::test]
async fn assess_cleanup_malformed_row_empty_tip_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Empty tip column — must not compare equal to any base_commit,
    // including an empty one; always fails closed.
    let branches = format!("{}\n", ferow("feature/x", "", "origin/feature/x", ""));
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

// ---------------------------------------------------------------
// Non-interactive PreservedUnpushed path. The non-interactive
// eprintln uses per-reason wording; this pins that the Unpushed
// arm is reached (FinalizeDecision::Preserved +
// CleanupStatus::PreservedUnpushed) when is_interactive=false.
// ---------------------------------------------------------------

#[tokio::test]
async fn unpushed_worktree_non_interactive_prints_warning_and_preserves() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status clean, branch ahead of base with no upstream → PreservedUnpushed
    let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

// ---------------------------------------------------------------
// Interactive prompt choices for PreservedUnpushed.
// Counterparts to the Dirty interactive tests; pins that the
// three-way prompt dispatch works for both preservation paths.
// ---------------------------------------------------------------

#[tokio::test]
async fn unpushed_branch_interactive_force_delete_runs_cleanup() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::DiscardAll]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree remove --force"))
    );
}

#[tokio::test]
async fn unpushed_branch_interactive_return_to_agent_signals_caller() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::ReturnToRole]));
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::ReturnToAgent);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

// ---------------------------------------------------------------
// Bare `gone` track annotation (no brackets). Some git versions
// emit `gone` instead of `[gone]`; both must short-circuit to Safe.
// ---------------------------------------------------------------

#[tokio::test]
async fn bare_gone_track_is_safe_to_delete() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Bare `gone` (no brackets) must also short-circuit to Safe.
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/x", "newhead", "origin/feature/x", "gone"),
    ]
    .join("\n")
        + "\n";
    // No rev-list expected — bare `gone` short-circuits just like `[gone]`.
    // symbolic-ref HEAD  (HEAD on feature/x → attached)
    let mut runner = fake_with_outputs(&["", &branches, "refs/heads/feature/x"]);
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
    assert!(
        !runner.recorded.iter().any(|c| c.contains("rev-list")),
        "bare gone short-circuit must not invoke rev-list; recorded={:?}",
        runner.recorded,
    );
}

// ---------------------------------------------------------------
// Detached-HEAD guard. Commits made in detached-HEAD mode don't
// appear in refs/heads/ and would slip past the branch loop.
// `symbolic-ref --quiet HEAD` is the sentinel: Err = detached (or
// git error); both are treated as unsafe unless rev-parse HEAD
// confirms HEAD is parked at base_commit.
// ---------------------------------------------------------------

#[tokio::test]
async fn detached_head_past_base_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // All named branches are safe (scratch at base), but HEAD is
    // detached and points at a commit past base_commit.
    let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
    // Queue: status, for-each-ref, rev-parse HEAD (symbolic-ref fails).
    let mut runner = fake_failing_capture(&["", &branches, "deadbeef"], "symbolic-ref");
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

#[tokio::test]
async fn detached_head_at_base_is_safe_to_delete() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Detached HEAD parked exactly at base_commit ("abc") — no
    // unreachable commits; safe to clean.
    let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
    // Queue: status, for-each-ref, rev-parse HEAD (= "abc\n" → trims to base_commit).
    // Using the real git rev-parse output format (trailing newline) so trim() is exercised.
    let mut runner = fake_failing_capture(&["", &branches, "abc\n"], "symbolic-ref");
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
}

#[tokio::test]
async fn has_jackin_sessions_error_treated_as_sessions_present() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient {
        fail_with: vec![("docker exec".to_owned(), "exec failed".to_owned())],
        ..Default::default()
    };
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::still_running(),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut r,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
}

#[tokio::test]
async fn detached_head_rev_parse_failure_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Both symbolic-ref and rev-parse fail → fail-closed.
    let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
    let mut runner = FakeRunner {
        capture_queue: VecDeque::from(vec![String::new(), branches]),
        fail_on: vec!["symbolic-ref".to_owned(), "rev-parse".to_owned()],
        ..FakeRunner::default()
    };
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        false,
        DirtyExitPolicy::Ask,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

// ---------------------------------------------------------------
// D8: dirty_exit_policy = keep / discard — no prompt needed.
// ---------------------------------------------------------------

/// `dirty_exit_policy = keep` preserves all dirty records without prompting.
#[tokio::test]
async fn keep_policy_preserves_dirty_record_without_prompt() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    // NoPrompt panics if called; keep-policy must never call the dialog.
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Keep,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
}

/// `dirty_exit_policy = discard` skips the dialog and attempts to force-delete
/// all dirty records. Whether the git cleanup succeeds depends on runner state,
/// but the dialog must never be called (`NoPrompt` would panic if it were).
#[tokio::test]
async fn discard_policy_skips_dialog_on_dirty_record() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    // NoPrompt panics if called; discard-policy must never call the dialog.
    let mut p = NoPrompt;
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Discard,
        &mut p,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    // Either Cleaned (git cleanup succeeded) or Preserved (git cleanup failed
    // because runner queue is empty). Key: ReturnToAgent must never happen.
    assert!(
        matches!(dec, FinalizeDecision::Cleaned | FinalizeDecision::Preserved),
        "discard policy must never return ReturnToAgent; got {dec:?}"
    );
}

#[test]
fn exit_action_prompt_reads_recorded_choice() {
    let dir = TempDir::new().expect("tempdir");
    let mut prompt = ExitActionPrompt {
        state_dir: dir.path().to_path_buf(),
    };
    // Absent file → KeepAll (never lose at-risk work).
    assert_eq!(
        prompt.ask_exit_dialog("c", &[]).expect("prompt"),
        ExitDialogChoice::KeepAll
    );
    // Discard recorded → DiscardAll.
    std::fs::write(dir.path().join("exit-action.json"), "\"discard\"").expect("write");
    assert_eq!(
        prompt.ask_exit_dialog("c", &[]).expect("prompt"),
        ExitDialogChoice::DiscardAll
    );
    // Keep recorded → KeepAll.
    std::fs::write(dir.path().join("exit-action.json"), "\"keep\"").expect("write");
    assert_eq!(
        prompt.ask_exit_dialog("c", &[]).expect("prompt"),
        ExitDialogChoice::KeepAll
    );
}

#[test]
fn read_exit_action_none_when_absent_or_garbage() {
    let dir = TempDir::new().expect("tempdir");
    assert_eq!(read_exit_action(dir.path()), None);
    std::fs::write(dir.path().join("exit-action.json"), "not json").expect("write");
    assert_eq!(read_exit_action(dir.path()), None);
}

#[tokio::test]
async fn exit_action_keep_preserves_via_finalize() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("exit-action.json"), "\"keep\"").unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut prompt = ExitActionPrompt { state_dir };
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut prompt,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    assert_eq!(
        read_records(dir.path()).unwrap()[0].cleanup_status,
        CleanupStatus::PreservedDirty
    );
}

#[tokio::test]
async fn exit_action_discard_cleans_via_finalize() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("exit-action.json"), "\"discard\"").unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut prompt = ExitActionPrompt { state_dir };
    let docker = jackin_runtime::runtime::test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(
        "jackin-x",
        dir.path(),
        AttachOutcome::stopped(0),
        true,
        DirtyExitPolicy::Ask,
        &mut prompt,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
}
