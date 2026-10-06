// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_grant_validation_failure_preserves_unadopted_dind() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.docker.grants = Some(jackin_core::DockerGrants {
        user: Some("root".to_owned()),
        sudo: Some(true),
        ..Default::default()
    });
    persist_test_config(&paths, &config);

    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );

    let prewarm_dind = "jk-prewarm-grants-dind";
    let prewarm_net = "jk-prewarm-grants-net";
    let prewarm_certs = "jk-prewarm-grants-certs";
    write_prewarmed_dind_state(
        &paths,
        &DindSidecarPrewarm {
            dind: prewarm_dind.to_owned(),
            dind_id: "prewarm-grants-dind-id".to_owned(),
            network: prewarm_net.to_owned(),
            certs_volume: prewarm_certs.to_owned(),
            ready_ms: 12,
            kept: true,
        },
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker
        .container_id_by_name
        .borrow_mut()
        .insert(prewarm_dind.to_owned(), "prewarm-grants-dind-id".to_owned());
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(prewarm_dind.to_owned(), ContainerState::Running);
    let mut network_labels = HashMap::new();
    network_labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
    network_labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    docker.inspect_network_queue.borrow_mut().push_back(Some(
        jackin_docker::docker_client::NetworkRow {
            name: prewarm_net.to_owned(),
            labels: network_labels,
        },
    ));
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());

    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        ..LoadOptions::default()
    };

    let result = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await;

    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("docker grants validation failed"),
        "unexpected error: {error:#}"
    );
    let recorded = docker.recorded.borrow();
    for operation in [
        format!("docker rm -f {prewarm_dind}"),
        format!("docker volume rm {prewarm_certs}"),
        format!("docker network rm {prewarm_net}"),
    ] {
        assert!(
            !recorded.contains(&operation),
            "invalid grants must not adopt or destroy prewarm: {recorded:?}"
        );
    }
    assert!(paths.data_dir.join("prewarm-dind.json").exists());
}

#[tokio::test]
async fn load_agent_does_not_short_circuit_on_running_instance() {
    // D13 reversal of PR #576: launch must NOT auto-attach to a live container.
    // The full build pipeline must run (`docker build`) even when a current-role
    // container is Running. Two inspect entries: the early-attach probe and the
    // in-pipeline probe both see Running and both reject it.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    std::fs::create_dir_all(&cached_repo.repo_dir).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nmodel = \"sonnet\"\n",
    )
    .unwrap();
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            accounts: vec!["test".to_owned()],
            workdir: "/workspace".to_owned(),
            mounts: repo_workspace(&cached_repo.repo_dir).mounts,
            default_agent: Some(jackin_core::Agent::Claude),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    persist_test_config(&paths, &config);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);
    let mut workspace = repo_workspace(&cached_repo.repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.default_agent = Some(jackin_core::Agent::Claude);
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker build "),
        "D13: build must run even when current-role container is running; recorded:\n{recorded}"
    );
    assert!(
        !recorded.starts_with(&format!("docker exec {container_name}")),
        "D13: launch must not auto-attach to running container; recorded:\n{recorded}"
    );
}

#[tokio::test]
async fn task_scoped_overrides_bypass_running_current_role_reuse() {
    task_overrides_launch_fresh_codex_for_current_state(ContainerState::Running).await;
}

#[tokio::test]
async fn task_scoped_overrides_bypass_stopped_current_role_reuse() {
    task_overrides_launch_fresh_codex_for_current_state(ContainerState::Stopped {
        exit_code: 137,
        oom_killed: false,
    })
    .await;
}

#[tokio::test]
async fn explicit_restore_rejects_agent_different_from_stored_instance() {
    let (_temp, paths, mut config, selector, workspace, docker, mut runner, current_container) =
        task_override_current_role_fixture(ContainerState::Running);
    let opts = LoadOptions {
        agent: Some(jackin_core::Agent::Claude),
        restore_container_base: Some(current_container),
        ..LoadOptions::default()
    };

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("does not match stored instance agent")
    );
    assert!(
        docker.recorded.borrow().is_empty(),
        "agent mismatch must fail before inspecting or starting a container"
    );
}

#[tokio::test]
async fn load_agent_attaches_explicit_restore_container_with_stored_agent_before_role_repo() {
    struct FailingOpRunner;

    impl jackin_env::OpRunner for FailingOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("explicit restore path should not resolve operator env")
        }

        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!("explicit restore path should not probe op")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.env.insert(
        "OPERATOR_RESTORE_SECRET".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/restore-secret".to_owned(),
            path: "Vault/Item/restore secret".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.workspace_name = None;
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: container_name.to_owned(),
        dind_container_id: manifest.docker.dind_container.clone(),
    });
    write_indexed_manifest(&paths, &manifest);
    provision_restore_account_policy(&paths, &config, &manifest);
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
        ])),
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            String::new(),
            "Sessions: 1\n".to_owned(),
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);
    let opts = LoadOptions {
        // The normal `jackin restore` path resolves this from the persisted
        // instance manifest. It is identity for the exact target, not an
        // incompatible agent override.
        agent: Some(jackin_core::Agent::Claude),
        op_runner: Some(Box::new(FailingOpRunner)),
        restore_container_base: Some(container_name.to_owned()),
        role_branch: Some("restore-ref".to_owned()),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker exec")
            && recorded.contains(container_name)
            && recorded.contains("jackin-capsule"),
        "explicit restore container must attach through Capsule; recorded:\n{recorded}"
    );
    for forbidden in [
        &cached_repo.repo_dir.display().to_string(),
        "docker build ",
        "gh auth token",
        "docker inspect image:",
        "docker run --rm --entrypoint",
    ] {
        assert!(
            !recorded.contains(forbidden),
            "explicit restore path must skip {forbidden}; recorded:\n{recorded}"
        );
    }
}
