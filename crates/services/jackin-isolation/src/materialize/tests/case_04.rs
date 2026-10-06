// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn adopt_aborts_when_worktree_prune_fails() {
    // Prune failure must short-circuit before `worktree add`,
    // otherwise the add proceeds against an inconsistent admin
    // index and risks corrupting state.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();
    write_loose_branch(repo.path(), "jackin/scratch/jackin-x", "abc123\n");
    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(),
        "",
        "true\n",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n",
    ]);
    runner.fail_on.push("worktree prune".into());

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
    assert!(err.to_string().contains("worktree prune"));
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree add")),
        "worktree add must not run when prune fails; got {:?}",
        runner.run_recorded,
    );
    assert!(
        read_records(&container_dir).unwrap().is_empty(),
        "no record on prune failure",
    );
}

#[tokio::test]
async fn fresh_materialization_uses_dash_b_when_branch_absent() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();
    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(),
        "",
        "true\n",
        "cafef00dcafef00dcafef00dcafef00dcafef00d\n",
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
    let add = runner
        .run_recorded
        .iter()
        .find(|c| c.contains("worktree add"))
        .expect("worktree add should have been invoked");
    assert!(
        add.split_whitespace().any(|t| t == "-b"),
        "fresh path must use -b; got {add}",
    );
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree prune")),
    );
    let recs = read_records(&container_dir).unwrap();
    assert_eq!(
        recs[0].base_commit,
        "cafef00dcafef00dcafef00dcafef00dcafef00d",
    );
}

#[tokio::test]
async fn docker_mount_order_is_length_ascending() {
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![
            MaterializedMount {
                bind_src: "/cache".into(),
                dst: "/workspace/proj/target".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
            MaterializedMount {
                bind_src: "/wt".into(),
                dst: "/workspace/proj".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: None,
            },
        ],
        keep_awake_enabled: false,
    };
    let ordered = mount_order_for_docker(&mat);
    assert_eq!(ordered[0].dst, "/workspace/proj");
    assert_eq!(ordered[1].dst, "/workspace/proj/target");
}

#[tokio::test]
async fn docker_mount_order_is_stable_for_same_length() {
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![
            MaterializedMount {
                bind_src: "/a".into(),
                dst: "/workspace/aa".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
            MaterializedMount {
                bind_src: "/b".into(),
                dst: "/workspace/bb".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
        ],
        keep_awake_enabled: false,
    };
    let ordered = mount_order_for_docker(&mat);
    assert_eq!(ordered[0].dst, "/workspace/aa");
    assert_eq!(ordered[1].dst, "/workspace/bb");
}

#[tokio::test]
async fn isolation_identity_uses_saved_stem_and_excludes_ad_hoc_labels() {
    for isolation in [MountIsolation::Worktree, MountIsolation::Clone] {
        for saved in [true, false] {
            let repo = make_repo_root();
            let data = tempfile::TempDir::new().unwrap();
            let container = "jk-a1b2c3d4-stem";
            let container_dir = data.path().join(container);
            let mut resolved = resolved_with_one_isolated(repo.path(), "/workspace/repo");
            resolved.name = "saved-stem".into();
            resolved.label = "/display/label".into();
            resolved.mounts[0].isolation = isolation;
            let name = WorkspaceName::parse("saved-stem").unwrap();
            let mut runner = if isolation == MountIsolation::Worktree {
                fake_with_outputs(&[&repo.path().to_string_lossy(), "", "", "0", "deadbeef"])
            } else {
                fake_with_outputs(&[&repo.path().to_string_lossy(), "", "deadbeef", ""])
            };
            materialize_workspace(
                &resolved,
                &container_dir,
                "role",
                container,
                saved.then_some(&name),
                &PreflightContext {
                    workspace_label: WorkspaceLabel::parse(&resolved.label).unwrap(),
                    force: false,
                    interactive: false,
                },
                &mut runner,
            )
            .await
            .unwrap();
            let records = read_records(&container_dir).unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].workspace_name.as_ref(), saved.then_some(&name));
            let found = crate::state::list_records_for_workspace(data.path(), &name).unwrap();
            assert_eq!(found.len(), usize::from(saved));
            assert!(
                crate::state::list_records_for_workspace(
                    data.path(),
                    &WorkspaceName::parse("display-label").unwrap()
                )
                .unwrap()
                .is_empty()
            );
        }
    }
}

#[tokio::test]
async fn isolation_reuse_rejects_another_saved_workspace_before_git() {
    for isolation in [MountIsolation::Worktree, MountIsolation::Clone] {
        let repo = make_repo_root();
        let state = tempfile::TempDir::new().unwrap();
        let mut resolved = resolved_with_one_isolated(repo.path(), "/workspace/repo");
        resolved.mounts[0].isolation = isolation;
        crate::state::write_records(
            state.path(),
            &[IsolationRecord {
                workspace_name: Some(WorkspaceName::parse("other-workspace").unwrap()),
                mount_dst: resolved.mounts[0].dst.clone(),
                original_src: resolved.mounts[0].src.clone(),
                isolation,
                worktree_path: String::new(),
                scratch_branch: String::new(),
                base_commit: "abc".into(),
                selector_key: "role".into(),
                container_name: "jk-a1b2c3d4-role".into(),
                cleanup_status: CleanupStatus::Active,
            }],
        )
        .unwrap();
        let mut runner = fake_with_outputs(&[]);
        let error = materialize_workspace(
            &resolved,
            state.path(),
            "role",
            "jk-a1b2c3d4-role",
            Some(&WorkspaceName::parse("saved-stem").unwrap()),
            &PreflightContext {
                workspace_label: WorkspaceLabel::parse("other-workspace").unwrap(),
                force: false,
                interactive: false,
            },
            &mut runner,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("different saved workspace"));
        assert!(runner.recorded.is_empty());
        assert!(runner.run_recorded.is_empty());
    }
}
