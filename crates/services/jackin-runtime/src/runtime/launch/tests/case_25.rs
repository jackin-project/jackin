// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn stopped_matching_instance_with_missing_network_recreates_current_role() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // When the network is missing, the captured role identity is retained for
    // ID-bound teardown before recreation.
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        inspect_network_queue: std::cell::RefCell::new(VecDeque::from([None])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(
        candidate,
        RestoreResolution::RecreateCurrentRoleWithHandle(
            jackin_core::ContainerHandle::new(container_name, container_name).unwrap(),
        )
    );
}

#[tokio::test]
async fn recreate_refuses_partial_role_and_dind_identity() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    let dind_name = manifest.docker.dind_container.clone().unwrap();
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::Running,
        ])),
        container_id_by_name: std::cell::RefCell::new(
            [(dind_name.clone(), "replacement-dind-id".to_owned())]
                .into_iter()
                .collect(),
        ),
        ..Default::default()
    };

    let error =
        super::launch_pipeline::teardown_recreate_container(&paths, container_name, None, &docker)
            .await
            .unwrap_err();

    assert!(
        error.to_string().contains("ownership identity unavailable"),
        "{error}"
    );
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm"))
    );
}

#[tokio::test]
async fn recreate_checks_recorded_sidecar_identity_before_mutation() {
    for actual_id in ["original-dind-id", "replacement-dind-id"] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let mut manifest = workspace_manifest(
            container_name,
            "agent-smith",
            "Agent Smith",
            jackin_core::Agent::Claude,
        );
        manifest.docker_identity = Some(crate::instance::DockerIdentity {
            role_container_id: "original-role-id".to_owned(),
            dind_container_id: Some("original-dind-id".to_owned()),
        });
        write_indexed_manifest(&paths, &manifest);
        let dind_name = manifest.docker.dind_container.clone().unwrap();
        let docker = jackin_test_support::FakeDockerClient {
            inspect_state_by_name: std::cell::RefCell::new(std::collections::HashMap::from([
                (container_name.to_owned(), ContainerState::NotFound),
                (dind_name.clone(), ContainerState::Running),
            ])),
            container_id_by_name: std::cell::RefCell::new(std::collections::HashMap::from([(
                dind_name,
                actual_id.to_owned(),
            )])),
            ..Default::default()
        };
        let result = super::launch_pipeline::teardown_recreate_container(
            &paths,
            container_name,
            None,
            &docker,
        )
        .await;
        if actual_id == "original-dind-id" {
            result.unwrap();
            assert_eq!(
                docker.bound_operations.borrow().as_slice(),
                ["remove:original-dind-id"]
            );
            assert!(
                docker
                    .recorded
                    .borrow()
                    .iter()
                    .any(|op| op == &format!("docker network rm {}", manifest.docker.network))
            );
        } else {
            let error = result.unwrap_err();
            assert!(
                error.to_string().contains("ownership identity mismatch"),
                "{error}"
            );
            assert!(docker.bound_operations.borrow().is_empty());
            assert!(
                !docker
                    .recorded
                    .borrow()
                    .iter()
                    .any(|op| op.starts_with("docker network rm"))
            );
        }
    }
}

#[tokio::test]
async fn related_restore_candidate_requires_rich_dialog_for_fresh_load() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // inspect -> NotFound -> matching but different role, but no rich
    // progress dialog is available in this direct unit-test call.
    let docker = jackin_test_support::FakeDockerClient::default();

    let error = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap_err();

    // The related-only case flows through the unified rich restore dialog
    // instead of silently starting fresh.
    let message = error.to_string();
    assert!(
        message.contains("rich launch dialog"),
        "unexpected error: {message}"
    );
    assert!(message.contains("agent-smith"), "{message}");
}

#[tokio::test]
async fn running_related_instance_does_not_block_fresh_load() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // Related container is Running → skip → StartFresh
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(candidate, RestoreResolution::StartFresh);
}

#[tokio::test]
async fn stopped_related_instance_does_not_block_fresh_load() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // Related container stopped non-cleanly → skip → StartFresh
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(candidate, RestoreResolution::StartFresh);
}

#[tokio::test]
async fn related_restore_candidates_ignore_finished_instances() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let mut manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::CleanExited);
    write_indexed_manifest(&paths, &manifest);
    // Manifest is CleanExited → not a restore candidate, no docker call
    let docker = jackin_test_support::FakeDockerClient::default();

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(candidate, RestoreResolution::StartFresh);
    assert!(docker.recorded.borrow().is_empty());
}

#[tokio::test]
async fn related_restore_candidate_with_container_recovers_in_place() {
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let candidate = RelatedRestoreCandidate {
        manifest: workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            jackin_core::Agent::Claude,
        ),
        docker_state: ContainerState::Running,
    };

    let resolution = recover_related_restore_candidate(&candidate).unwrap();

    assert_eq!(
        resolution,
        RestoreResolution::RecoverRelatedRole(container_name.to_owned())
    );
}

#[tokio::test]
async fn missing_related_restore_candidate_rebuilds_in_place() {
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let candidate = RelatedRestoreCandidate {
        manifest: workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            jackin_core::Agent::Claude,
        ),
        docker_state: ContainerState::NotFound,
    };

    let resolution = recover_related_restore_candidate(&candidate).unwrap();

    assert!(matches!(
        resolution,
        RestoreResolution::RebuildRelatedRole(ref manifest)
            if manifest.container_base == container_name
    ));
}

#[tokio::test]
async fn related_restore_load_options_use_manifest_source_ref_and_agent() {
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let mut manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Codex,
    );
    manifest.agent_runtime = "codex".to_owned();
    manifest.role_source_ref = Some("restore-ref".to_owned());
    let current = LoadOptions::for_load(true, false);

    let opts = related_restore_load_options(&current, &manifest).unwrap();

    assert!(opts.debug);
    assert_eq!(opts.agent, Some(jackin_core::Agent::Codex));
    assert_eq!(opts.role_branch.as_deref(), Some("restore-ref"));
    assert_eq!(opts.restore_container_base.as_deref(), Some(container_name));
    assert_eq!(
        opts.restore_role_source_git.as_deref(),
        Some("https://example.invalid/the-architect.git")
    );
}

#[tokio::test]
async fn supersede_restore_candidates_updates_manifest_and_index() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);

    supersede_restore_candidates(&paths, vec![manifest]).unwrap();

    let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
    assert_eq!(manifest.status, InstanceStatus::Superseded);
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(index.instances[0].status, InstanceStatus::Superseded);
}

#[tokio::test]
async fn restore_candidate_label_includes_manifest_and_mount_state() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Codex,
    );
    manifest.mark_status(InstanceStatus::PreservedDirty);
    manifest.last_attach_outcome = Some("exit:137".into());
    crate::isolation::state::write_records(
        &paths.data_dir.join(container_name),
        &[crate::isolation::state::IsolationRecord {
            workspace_name: Some(jackin_core::WorkspaceName::parse("workspace").unwrap()),
            mount_dst: "/workspace".into(),
            original_src: "/host/workspace".into(),
            isolation: MountIsolation::Worktree,
            worktree_path: "/tmp/worktree".into(),
            scratch_branch: "jackin/test".into(),
            base_commit: "abc123".into(),
            selector_key: "agent-smith".into(),
            container_name: container_name.into(),
            cleanup_status: crate::isolation::state::CleanupStatus::PreservedDirty,
        }],
    )
    .unwrap();

    let label = restore_candidate_label(&paths, &manifest);

    assert!(label.contains("k7p9m2xq"), "{label}");
    assert!(label.contains("status:preserved_dirty"), "{label}");
    assert!(label.contains("agent:codex"), "{label}");
    assert!(label.contains("role:agent-smith"), "{label}");
    assert!(label.contains("mounts:1 dirty:1 unpushed:0"), "{label}");
    assert!(label.contains("attach:exit:137"), "{label}");
    assert!(!label.contains(container_name), "{label}");
}
