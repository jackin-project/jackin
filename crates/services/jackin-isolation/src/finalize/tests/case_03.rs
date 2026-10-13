// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
async fn pruned_gone_branch_is_treated_as_merged_and_cleaned() {
    // Policy: `[gone]` upstream means merged-and-pruned (squash-merge is the
    // dominant workflow); assess issues zero rev-list and treats the branch
    // as clean. Pinned by jackin-core
    // `upstream_gone_is_treated_as_merged_clean`.
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
        !runner.recorded.iter().any(|c| c.contains("rev-list")),
        "gone branch must not issue reachability queries; recorded={:?}",
        runner.recorded,
    );
}

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
    // Queue: status, for-each-ref, rev-list for feature/b only
    // (feature/a is [gone], which production treats as
    // merged-and-pruned with no reachability query), symbolic-ref.
    let mut runner = fake_with_outputs(&["", &branches, "", "refs/heads/feature/b"]);
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
    let revlist_calls = runner
        .recorded
        .iter()
        .filter(|c| c.contains("rev-list"))
        .count();
    assert_eq!(
        revlist_calls, 1,
        "only the live-upstream branch issues a reachability query; \
         the [gone] branch is accepted as merged-and-pruned without one. recorded={:?}",
        runner.recorded,
    );
}

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
async fn unpushed_branch_prompts_with_unpushed_reason() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
    // status clean → for-each-ref → ahead+no-upstream → preserve
    let mut runner = fake_with_outputs(&["", &branches]);
    let mut p = RecordingPrompt::new(ExitDialogChoice::KeepAll); // operator picks "preserve"
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
    assert_eq!(p.seen, vec![PreservedReason::Unpushed]);
}

#[tokio::test]
async fn dirty_worktree_prompts_with_dirty_reason() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    let mut runner = fake_with_outputs(&[" M file\n"]);
    let mut p = RecordingPrompt::new(ExitDialogChoice::KeepAll);
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
    assert_eq!(p.seen, vec![PreservedReason::Dirty]);
}

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
async fn unpushed_worktree_non_interactive_prints_warning_and_preserves() {
    let dir = TempDir::new().unwrap();
    let r = rec(dir.path());
    std::fs::create_dir_all(&r.original_src).unwrap();
    write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
    // status clean, branch ahead of base with no upstream → PreservedUnpushed
    let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
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
