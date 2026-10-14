// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::state::read_records;

#[tokio::test]
async fn held_record_is_preserved_despite_discard_policy() {
    let dir = TempDir::new().unwrap();
    let record = rec(dir.path());
    write_records(dir.path(), std::slice::from_ref(&record)).unwrap();
    crate::holds::hold_work(
        dir.path(),
        &record.worktree_path,
        "checkpoint capture",
        None,
    )
    .unwrap();

    // Interactive Ask surfaces the held record in the exit dialog; even an
    // explicit DiscardAll answer must not remove it.
    let mut prompt = RecordingPrompt::new(ExitDialogChoice::DiscardAll);
    let mut runner = FakeRunner::default();
    let docker = jackin_test_support::FakeDockerClient::default();
    let decision = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-x",
        container_state_dir: dir.path(),
        outcome: AttachOutcome::Stopped(0),
        is_interactive: true,
        dirty_exit_policy: DirtyExitPolicy::Ask,
        prompt: &mut prompt,
        docker: &docker,
        runner: &mut runner,
        container: ContainerHandle::new("jackin-x", "test-finalizer-id").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(decision, FinalizeDecision::Preserved);
    assert_eq!(prompt.seen, vec![PreservedReason::Held]);
    // Source and record remain available.
    assert!(Path::new(&record.worktree_path).exists());
    assert_eq!(read_records(dir.path()).unwrap().len(), 1);
}
