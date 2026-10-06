// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn first_materialization_runs_worktree_add_and_writes_record() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-the-architect");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    // capture queue (in order materialize_workspace will request):
    //   preflight: rev-parse --show-toplevel
    //   preflight: status --porcelain
    //   ensure_worktree_config: extensions.worktreeConfig --get
    //   ensure_worktree_config: core.repositoryformatversion --get
    //   rev-parse HEAD
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel (preflight)
        "",                             // status --porcelain (clean)
        "",                             // extensions.worktreeConfig --get (not enabled)
        "0",                            // core.repositoryformatversion --get
        "deadbeef\n",                   // rev-parse HEAD
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
    let m = &mat.mounts[0];
    assert!(
        m.bind_src
            .contains("/git/worktree/repo/workspace/jackin/jackin-the-architect"),
        "worktree subdir basename = container name, under git/worktree/repo/<dst-tree>/; got {}",
        m.bind_src
    );
    assert_eq!(m.dst, "/workspace/jackin");
    assert_eq!(m.isolation, MountIsolation::Worktree);

    // git worktree add should have been invoked.
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree add"))
    );

    // record persisted; branch follows Model B (container name verbatim).
    let recs = read_records(&container_dir).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].cleanup_status, CleanupStatus::Active);
    assert_eq!(recs[0].base_commit, "deadbeef");
    assert_eq!(
        recs[0].scratch_branch,
        "jackin/scratch/jackin-the-architect"
    );
}

#[tokio::test]
async fn shared_mounts_pass_through_unchanged() {
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();
    let resolved = ResolvedWorkspace {
        name: String::new(),
        label: "jackin".into(),
        workdir: "/workspace/x".into(),
        mounts: vec![MountConfig {
            src: "/tmp/cache".into(),
            dst: "/workspace/cache".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: MountHealReport::default(),
    };
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
    assert_eq!(mat.mounts[0].bind_src, "/tmp/cache");
    assert_eq!(mat.mounts[0].isolation, MountIsolation::Shared);
    assert!(
        runner.run_recorded.is_empty(),
        "no git ops for shared mounts"
    );
}

#[tokio::test]
async fn clone_materialization_runs_local_shared_clone_and_writes_record() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "https://github.com/jackin-project/jackin\n", // remote get-url origin
    ]);

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

    assert_eq!(mat.mounts[0].isolation, MountIsolation::Clone);
    assert!(mat.mounts[0].worktree_aux.is_none());
    assert!(
        mat.mounts[0]
            .bind_src
            .contains("/git/clone/repo/workspace/jackin/jackin-x"),
        "got {}",
        mat.mounts[0].bind_src,
    );
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("git clone --local"))
    );
    assert!(
        !runner.run_recorded.iter().any(|c| c.contains("checkout")),
        "clone mode must not create or switch to a scratch branch: {:?}",
        runner.run_recorded
    );
    // Origin rewritten from bind-mount loopback to host's upstream.
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("remote set-url origin https://github.com/jackin-project/jackin")),
        "expected `git remote set-url origin <upstream>` in: {:?}",
        runner.run_recorded
    );
    let recs = read_records(&container_dir).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].isolation, MountIsolation::Clone);
    assert_eq!(recs[0].base_commit, "deadbeef");
    assert_eq!(recs[0].scratch_branch, "");
}

#[tokio::test]
async fn clone_materialization_normalizes_ssh_origin_to_https() {
    // SCP-form host origin would point the clone at an SSH endpoint
    // the container has no key for; rewrite must land on HTTPS so
    // `gh auth git-credential` can authenticate the push.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "git@github.com:jackin-project/jackin.git\n", // remote get-url origin (SCP form)
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

    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c
                .contains("remote set-url origin https://github.com/jackin-project/jackin.git")),
        "expected SCP-form origin to be normalized to HTTPS in: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn clone_materialization_skips_origin_rewrite_when_host_origin_is_empty() {
    // Ok-arm with whitespace-only output: `trimmed.is_empty()`
    // collapses to `None`, no rewrite emitted.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "   \r\n\t  ",                  // remote get-url origin (whitespace-only)
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

    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("remote set-url")),
        "expected no `git remote set-url` when host origin is empty; got: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn clone_materialization_falls_through_when_host_has_no_origin_remote() {
    // Err with `No such remote 'origin'` (fresh init, never pushed)
    // — fall through to loopback, do not abort.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
    ]);
    runner.fail_with.push((
        "remote get-url origin".into(),
        "fatal: No such remote 'origin'".into(),
    ));

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
    .expect("legitimate `No such remote` should fall through, not abort");

    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("remote set-url")),
        "expected no `git remote set-url` when host has no origin; got: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn clone_materialization_aborts_when_get_url_fails_unexpectedly() {
    // Permission denied / corrupt config — anything that isn't a
    // `No such remote` signal — must abort the launch rather than
    // silently fall through to a loopback origin and misroute the
    // operator's pushes.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
    ]);
    runner.fail_with.push((
        "remote get-url origin".into(),
        "fatal: unable to read config: Permission denied".into(),
    ));

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
    .expect_err("unexpected get-url failure should abort the launch");

    let chain = format!("{err:#}");
    assert!(
        chain.contains("failed to read host repo")
            && chain.contains("loopback origin")
            && chain.contains("misroute pushes"),
        "abort message should explain the failure mode and remediation; got: {chain}",
    );
}
