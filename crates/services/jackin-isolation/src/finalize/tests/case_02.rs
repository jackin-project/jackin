// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn dirty_worktree_non_interactive_prints_warning_and_preserves() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut p = NoPrompt;
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: false,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
}

#[tokio::test]
async fn assess_cleanup_status_capture_failure_preserves_unpushed() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status --porcelain errors → must NOT be treated as clean tree.
    let mut runner = fake_failing_capture(&[], "status --porcelain");
    let mut p = NoPrompt;
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: false,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    // Preservation must not enter pinned repository cleanup.
    assert!(
        !runner
            .run_options
            .iter()
            .any(|opts| opts.pinned_cwd.is_some()),
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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: false,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    assert!(
        !runner
            .run_options
            .iter()
            .any(|opts| opts.pinned_cwd.is_some())
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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: false,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    assert!(
        !runner
            .run_options
            .iter()
            .any(|opts| opts.pinned_cwd.is_some()),
        "rev-list failure must not auto-delete; recorded={:?}",
        runner.run_recorded,
    );
}

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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(
        read_records(dir.path()).unwrap().is_empty(),
        "both records should be removed after force-delete on both",
    );
    // Each journal cleanup issues 6 pinned invocations (scratch-tip
    // inspections, ref-format probes, and verification re-reads).
    let removes = runner
        .run_options
        .iter()
        .filter(|opts| opts.pinned_cwd.is_some())
        .count()
        / 6;
    assert_eq!(
        removes, 2,
        "must verify cleanup for BOTH preserved mounts; recorded={:?}",
        runner.run_recorded
    );
}

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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::ReturnToAgent);
    // No worktrees removed — all three records still on disk.
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs.len(), 3, "ReturnToAgent must not delete any records");
    // Each journal cleanup issues 6 pinned invocations; zero are
    // expected here.
    let removes = runner
        .run_options
        .iter()
        .filter(|opts| opts.pinned_cwd.is_some())
        .count()
        / 6;
    assert_eq!(
        removes, 0,
        "ReturnToAgent must perform no cleanup; recorded={:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn multi_mount_cleanup_failure_in_loop_does_not_abort() {
    let dir = TempDir::new().unwrap();
    let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
    let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
    std::fs::create_dir_all(&r1.original_src).unwrap();
    write_records(dir.path(), &[r1, r2]).unwrap();
    // Both records assess to PreservedDirty (status returns dirty
    // for each), then force_cleanup_isolated runs git commands.
    // First mount's malformed exact-ref inspection retains its record;
    // the second mount's branch is positively verified absent.
    // Pre-fix: that bail would propagate via `?` and the second
    // record would never be prompted.
    let mut runner = FakeRunner {
        // Capture queue: status for r1, status for r2, then verify
        // capture for r1 (says branch IS present — triggers bail),
        // then verify capture for r2 (says branch absent — proceed).
        capture_queue: VecDeque::from([
            " M f1\n".to_owned(),
            " M f2\n".to_owned(),
            "malformed-ref-row\n".to_owned(),
            String::new(),
            "files".to_owned(),
            "sha1".to_owned(),
            String::new(),
        ]),
        // No mutation is authorized by the malformed first observation.
        ..FakeRunner::default()
    };
    // Operator force-deletes both.
    let mut p = ScriptedPrompt(VecDeque::from([ExitDialogChoice::DiscardAll]));
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
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

#[tokio::test]
async fn multi_mount_non_interactive_marks_all_preserved() {
    let dir = TempDir::new().unwrap();
    let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
    let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
    std::fs::create_dir_all(&r1.original_src).unwrap();
    write_records(dir.path(), &[r1, r2]).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n", " M file\n"]);
    let mut p = NoPrompt;
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: false,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: false,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Preserved);
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}
