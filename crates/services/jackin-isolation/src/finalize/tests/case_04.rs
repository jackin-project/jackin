// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn unpushed_branch_interactive_force_delete_runs_cleanup() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
    let mut runner = fake_with_outputs(&["", &branches]);
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
    assert!(read_records(dir.path()).unwrap().is_empty());
    assert!(
        runner
            .run_options
            .iter()
            .any(|opts| opts.pinned_cwd.is_some())
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
    let recs = read_records(dir.path()).unwrap();
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
}

#[tokio::test]
async fn bare_gone_track_with_remote_reachability_is_safe_to_delete() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // Bare `gone` is accepted like `[gone]`: merged-and-pruned, no queries.
    let branches = [
        ferow("jackin/scratch/x", "abc", "", ""),
        ferow("feature/x", "newhead", "origin/feature/x", "gone"),
    ]
    .join("\n")
        + "\n";
    // Queue: status, for-each-ref, symbolic-ref (attached). No rev-list:
    // production treats `gone` as merged-and-pruned without a
    // reachability query (see jackin-core assess_worktree).
    let mut runner = fake_with_outputs(&["", &branches, "refs/heads/feature/x"]);
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
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
    assert!(
        !runner.recorded.iter().any(|c| c.contains("rev-list")),
        "bare gone must not issue reachability queries; recorded={:?}",
        runner.recorded,
    );
}

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
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
}

#[tokio::test]
async fn has_jackin_sessions_error_treated_as_sessions_present() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_test_support::FakeDockerClient {
        fail_with: vec![("docker exec".to_owned(), "exec failed".to_owned())],
        ..Default::default()
    };
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::still_running(),
        is_interactive: false,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut p,
        docker: &docker,
        runner: &mut r,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
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

#[tokio::test]
async fn keep_policy_preserves_dirty_record_without_prompt() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    // NoPrompt panics if called; keep-policy must never call the dialog.
    let mut p = NoPrompt;
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Keep,
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
async fn discard_policy_skips_dialog_on_dirty_record() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    // NoPrompt panics if called; discard-policy must never call the dialog.
    let mut p = NoPrompt;
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Discard,
        prompt: &mut p,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut prompt,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
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
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut prompt,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(dir.path()).unwrap().is_empty());
}
