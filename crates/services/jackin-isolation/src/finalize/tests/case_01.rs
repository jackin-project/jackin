// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn rich_exit_dialog_keeps_all_when_rich_dialog_is_unavailable() {
    use crate::MountIsolation;
    use crate::state::{CleanupStatus, IsolationRecord};
    let mut prompt = RichCleanupPrompt;
    let record = IsolationRecord {
        workspace_name: Some(jackin_core::WorkspaceName::parse("test").unwrap()),
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

#[tokio::test]
async fn still_running_with_zero_sessions_cleans() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_test_support::FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from(["Sessions: 0\n".to_owned()])),
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
    assert_eq!(dec, FinalizeDecision::Cleaned);
}

#[tokio::test]
async fn still_running_with_unparseable_status_preserves_records() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_test_support::FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([String::new()])),
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
async fn still_running_with_sessions_preserves() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_test_support::FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            "Sessions: 1\n  [3] work (claude) state=working active=true\n".to_owned(),
        ])),
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
async fn stopped_non_zero_preserves_records() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::stopped(137),
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
async fn oom_killed_preserves_records() {
    let dir = TempDir::new().unwrap();
    let mut p = NoPrompt;
    let mut r = FakeRunner::default();
    let docker = jackin_test_support::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::oom_killed(),
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
        runner
            .run_options
            .iter()
            .any(|opts| opts.pinned_cwd.is_some())
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|c| c.contains("worktree remove") || c.contains("branch -D"))
    );
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
async fn dirty_worktree_interactive_preserve_choice_keeps_state() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
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
async fn dirty_worktree_interactive_return_to_agent_signals_caller() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
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
    assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
}
