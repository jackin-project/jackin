// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn clone_materialization_strips_embedded_credentials_from_host_origin() {
    // Credentialed host origin (PAT baked into `.git/config`) must
    // not land verbatim in the per-container clone.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "https://x-access-token:ghp_secretsecretsecret@github.com/jackin-project/jackin.git\n",
    ]);

    materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        Some(&WorkspaceName::parse("jackin").unwrap()),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    let setting = runner
        .run_recorded
        .iter()
        .find(|c| c.contains("remote set-url"))
        .expect("set-url should still run with a credential-stripped URL");
    assert!(
        setting.contains("https://github.com/jackin-project/jackin.git"),
        "set-url should target the credential-stripped URL; got: {setting}",
    );
    assert!(
        !setting.contains("ghp_secretsecretsecret") && !setting.contains("x-access-token"),
        "credentials must not appear in the set-url command; got: {setting}",
    );
}

#[tokio::test]
async fn clone_reuse_skips_git_ops_when_git_dir_exists() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let dst = "/workspace/jackin";
    let cp = clone_path_for(&container_dir, dst, "jackin-x");
    std::fs::create_dir_all(cp.join(".git")).unwrap();
    crate::state::write_records(
        &container_dir,
        std::slice::from_ref(&IsolationRecord {
            workspace_name: Some(WorkspaceName::parse("jackin").unwrap()),
            mount_dst: dst.into(),
            original_src: repo.path().to_string_lossy().into(),
            isolation: MountIsolation::Clone,
            worktree_path: cp.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/jackin-x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }),
    )
    .unwrap();

    let resolved = resolved_with_one_clone(repo.path(), dst);
    let mut runner = FakeRunner::default();
    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        Some(&WorkspaceName::parse("jackin").unwrap()),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(mat.mounts[0].bind_src, cp.to_string_lossy());
    assert!(runner.run_recorded.is_empty(), "no git ops on clone reuse");
}

#[tokio::test]
async fn second_materialization_with_existing_record_skips_git_ops() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let dst = "/workspace/jackin";
    let wt_path = worktree_path_for(&container_dir, dst, "jackin-x");
    std::fs::create_dir_all(&wt_path).unwrap();
    std::fs::write(wt_path.join(".git"), "gitdir: /elsewhere").unwrap();
    crate::state::write_records(
        &container_dir,
        std::slice::from_ref(&IsolationRecord {
            workspace_name: Some(WorkspaceName::parse("jackin").unwrap()),
            mount_dst: dst.into(),
            original_src: repo.path().to_string_lossy().into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt_path.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }),
    )
    .unwrap();

    let resolved = resolved_with_one_isolated(repo.path(), dst);
    let mut runner = FakeRunner::default();
    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        Some(&WorkspaceName::parse("jackin").unwrap()),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(mat.mounts[0].bind_src, wt_path.to_string_lossy());
    assert!(runner.run_recorded.is_empty(), "no git ops on reuse");
}

#[tokio::test]
async fn drift_when_recorded_src_differs_errors_before_git_ops() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let dst = "/workspace/jackin";
    let wt_path = worktree_path_for(&container_dir, dst, "jackin-x");
    std::fs::create_dir_all(&wt_path).unwrap();
    crate::state::write_records(
        &container_dir,
        std::slice::from_ref(&IsolationRecord {
            workspace_name: Some(WorkspaceName::parse("jackin").unwrap()),
            mount_dst: dst.into(),
            original_src: "/different/src".into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt_path.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }),
    )
    .unwrap();

    let resolved = resolved_with_one_isolated(repo.path(), dst);
    let mut runner = FakeRunner::default();
    let err = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        Some(&WorkspaceName::parse("jackin").unwrap()),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("source drift") || msg.contains("differs"));
    assert!(msg.contains("/different/src"));
    assert!(runner.run_recorded.is_empty(), "no git ops on drift error");
}

#[tokio::test]
async fn find_local_branch_tip_reads_loose_ref_sha() {
    let repo = make_repo_root();
    let path = repo.path().to_string_lossy();
    assert_eq!(find_local_branch_tip(&path, "jackin/scratch/x"), None);
    write_loose_branch(repo.path(), "jackin/scratch/x", "deadbeefcafe\n");
    assert_eq!(
        find_local_branch_tip(&path, "jackin/scratch/x"),
        Some("deadbeefcafe".into()),
    );
}

#[tokio::test]
async fn find_local_branch_tip_reads_packed_refs_sha() {
    let repo = make_repo_root();
    write_packed_refs(
        repo.path(),
        "# pack-refs with: peeled fully-peeled sorted\n\
             1111111111111111111111111111111111111111 refs/heads/main\n\
             2222222222222222222222222222222222222222 refs/heads/jackin/scratch/x\n\
             ^abcd1234abcd1234abcd1234abcd1234abcd1234\n",
    );
    let path = repo.path().to_string_lossy();
    assert_eq!(
        find_local_branch_tip(&path, "jackin/scratch/x"),
        Some("2222222222222222222222222222222222222222".into()),
    );
    assert_eq!(find_local_branch_tip(&path, "jackin/scratch/missing"), None);
}

#[tokio::test]
async fn find_local_branch_tip_loose_ref_wins_over_packed_refs() {
    // git semantics: loose refs override packed-refs entries.
    // Critical because base_commit feeds finalize's safety
    // classifier, and a wrong SHA there can authorize deletion
    // of operator work.
    let repo = make_repo_root();
    write_loose_branch(repo.path(), "jackin/scratch/x", "1010101010101010\n");
    write_packed_refs(
        repo.path(),
        "9999999999999999999999999999999999999999 refs/heads/jackin/scratch/x\n",
    );
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        Some("1010101010101010".into()),
    );
}

#[tokio::test]
async fn find_local_branch_tip_rejects_symref_loose_content() {
    // `git symbolic-ref refs/heads/<x> refs/heads/main` writes
    // `ref: refs/heads/main\n`. Returning that verbatim as the
    // SHA poisons IsolationRecord.base_commit.
    let repo = make_repo_root();
    write_loose_branch(repo.path(), "jackin/scratch/x", "ref: refs/heads/main\n");
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        None,
    );
}

#[tokio::test]
async fn find_local_branch_tip_empty_loose_falls_through_to_packed() {
    // A 0-byte ref file (interrupted git op, third-party
    // tooling) must not yield Some("") and must not block the
    // packed-refs lookup.
    let repo = make_repo_root();
    write_loose_branch(repo.path(), "jackin/scratch/x", "");
    write_packed_refs(
        repo.path(),
        "abcdef1234567890abcdef1234567890abcdef12 refs/heads/jackin/scratch/x\n",
    );
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        Some("abcdef1234567890abcdef1234567890abcdef12".into()),
    );
}

#[tokio::test]
async fn find_local_branch_tip_skips_malformed_packed_refs_lines() {
    let repo = make_repo_root();
    write_packed_refs(
        repo.path(),
        "# header only\n\
             ^abcd\n\
             noseparator\n\
             1111111111111111111111111111111111111111\trefs/heads/jackin/scratch/x\n",
    );
    // Tab-separated row also resolves (split_once now matches
    // any ASCII whitespace, defensive against non-stock writers).
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        Some("1111111111111111111111111111111111111111".into()),
    );
}

#[tokio::test]
async fn stale_scratch_branch_is_adopted_when_record_absent() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-the-architect");
    std::fs::create_dir_all(&container_dir).unwrap();

    write_loose_branch(
        repo.path(),
        "jackin/scratch/jackin-the-architect",
        "feedbeefcafebabefeedbeefcafebabefeedbeef\n",
    );

    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    // fake_with_outputs is positional: order must match
    // materialize_workspace's runner.capture() sequence.
    // Adopted branch tip is read directly from the loose ref —
    // no runner.capture() entry is consumed for it.
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(),
        "",
        "true\n",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n",
    ]);

    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "the-architect",
        "jackin-the-architect",
        Some(&WorkspaceName::parse("jackin").unwrap()),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(mat.mounts.len(), 1);

    let prune_idx = runner
        .run_recorded
        .iter()
        .position(|c| c.contains("worktree prune"))
        .expect("worktree prune should be invoked on adopt");
    let add_idx = runner
        .run_recorded
        .iter()
        .position(|c| c.contains("worktree add"))
        .expect("worktree add should be invoked");
    assert!(
        prune_idx < add_idx,
        "prune must run before add; got prune@{prune_idx} add@{add_idx}: {:?}",
        runner.run_recorded,
    );
    let add = &runner.run_recorded[add_idx];
    assert!(
        !add.split_whitespace().any(|t| t == "-b" || t == "--branch"),
        "adopt path must not pass -b/--branch; got {add}",
    );
    assert!(
        add.ends_with(" jackin/scratch/jackin-the-architect"),
        "adopt add must end with the existing branch as the last positional arg; got {add}",
    );

    let recs = read_records(&container_dir).unwrap();
    assert_eq!(recs.len(), 1);
    // base_commit is host_head, NOT branch tip — see the comment
    // above the adopt arm in materialize_one. Asserting the
    // branch-tip value here would silently re-introduce the
    // data-loss regression flagged in PR #219 review.
    assert_eq!(
        recs[0].base_commit,
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    );
    assert_eq!(
        recs[0].scratch_branch,
        "jackin/scratch/jackin-the-architect",
    );
}
