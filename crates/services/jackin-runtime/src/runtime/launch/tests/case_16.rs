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
async fn explicit_restore_rejects_invalid_matching_v7_tuple_before_docker_or_credential_writes() {
    let (_temp, paths, mut config, selector, workspace, docker, mut runner, current_container) =
        task_override_current_role_fixture(ContainerState::Running);
    config.env.insert(
        "OPERATOR_RESTORE_SECRET".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/restore-secret".to_owned(),
            path: "Vault/Item/restore secret".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let root = paths.data_dir.join(&current_container);
    let mut manifest = InstanceManifest::read(&root).unwrap();
    manifest.set_admitted_instances([jackin_instance::AdmittedInstance::new(
        "codex-main",
        jackin_core::Agent::Codex,
        "other-account",
    )]);
    write_indexed_manifest(&paths, &manifest);
    provision_restore_account_policy(&paths, &config, &manifest);

    let credential_path = root
        .join("credentials")
        .join(jackin_protocol::account_credentials_filename("codex-main"));
    std::fs::create_dir_all(credential_path.parent().unwrap()).unwrap();
    std::fs::write(&credential_path, b"preserve existing credentials").unwrap();
    let config_bytes = std::fs::read(&paths.config_file).unwrap();
    let fingerprint_bytes = std::fs::read(root.join("account-config.sha256")).unwrap();
    let admission_bytes = std::fs::read(root.join("account-admission.sha256")).unwrap();
    let credential_bytes = std::fs::read(&credential_path).unwrap();

    struct UnexpectedOpRunner;

    impl jackin_env::OpRunner for UnexpectedOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("invalid saved selection must fail before resolving operator env")
        }

        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!("invalid saved selection must fail before probing operator env")
        }
    }

    let opts = LoadOptions {
        agent: Some(jackin_core::Agent::Codex),
        restore_container_base: Some(current_container),
        op_runner: Some(Box::new(UnexpectedOpRunner)),
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
    let rendered = format!("{error:#}");

    assert!(
        rendered.contains("saved account selection is no longer valid")
            && rendered.contains("saved configuration"),
        "unexpected matching-v7 refusal: {rendered}"
    );
    assert!(
        !docker.recorded.borrow().iter().any(|call| {
            [
                "docker start",
                "docker run",
                "docker exec",
                "docker rm -f",
                "docker stop",
            ]
            .iter()
            .any(|forbidden| call.contains(forbidden))
        }) && docker.created_containers.borrow().is_empty(),
        "invalid saved tuple must not start/reconnect/create/remove a container: {:?}",
        docker.recorded.borrow()
    );
    assert!(
        !runner.recorded.iter().any(|call| {
            [
                "docker run",
                "docker start",
                "docker exec",
                "docker create",
                "docker rm -f",
                "docker stop",
                "jackin-capsule",
            ]
            .iter()
            .any(|forbidden| call.contains(forbidden))
        }),
        "invalid saved tuple must not launch Docker or stage Capsule credentials: {}",
        runner.recorded.join("\n")
    );
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), config_bytes);
    assert_eq!(
        std::fs::read(root.join("account-config.sha256")).unwrap(),
        fingerprint_bytes
    );
    assert_eq!(
        std::fs::read(root.join("account-admission.sha256")).unwrap(),
        admission_bytes
    );
    assert_eq!(std::fs::read(&credential_path).unwrap(), credential_bytes);
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
    write_singleton_claude_admission(&paths);
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
    manifest.set_admitted_instances([jackin_instance::AdmittedInstance::new(
        "claude-main",
        jackin_core::Agent::Claude,
        "test",
    )]);
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

#[tokio::test]
async fn absent_related_rebuild_launches_with_model_and_effort() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nmodel = \"sonnet\"\n",
    )
    .unwrap();
    std::fs::write(cached_repo.repo_dir.join(".git").join("FETCH_HEAD"), "\n").unwrap();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.workspace_name = None;
    manifest.set_admitted_instances([jackin_instance::AdmittedInstance::new(
        "claude-main",
        jackin_core::Agent::Claude,
        "test",
    )]);
    write_indexed_manifest(&paths, &manifest);
    provision_restore_account_policy(&paths, &config, &manifest);
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(container_name.to_owned(), ContainerState::NotFound);
    let mut runner = FakeRunner::for_load_agent([
        "https://example.invalid/agent-smith.git".to_owned(),
        String::new(),
        "false 0 false".to_owned(),
        "false 0 false".to_owned(),
    ]);
    let current = LoadOptions {
        model: Some("gpt-6-luna".to_owned()),
        effort: Some(jackin_core::ReasoningEffort::Max),
        ..LoadOptions::default()
    };
    let opts = related_restore_load_options(&current, &manifest).unwrap();

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
    .unwrap_or_else(|error| panic!("{error:#}\nrecorded:\n{}", runner.recorded.join("\n")));

    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config: jackin_protocol::CapsuleConfig =
        toml::from_str(&std::fs::read_to_string(&capsule_config_path).unwrap()).unwrap();
    assert_eq!(
        capsule_config.models.get("claude-main").map(String::as_str),
        Some("gpt-6-luna")
    );
    assert_eq!(
        capsule_config
            .efforts
            .get("claude-main")
            .map(String::as_str),
        Some("max")
    );
    assert!(
        runner.recorded.iter().any(|call| {
            call.contains("docker run -d --name ") && call.contains(container_name)
        }),
        "absent related rebuild must launch the saved container name; recorded:\n{}",
        runner.recorded.join("\n")
    );
}

#[tokio::test]
async fn existing_explicit_restore_still_rejects_model_and_effort() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let container_name = "jk-existing-role";
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(container_name.to_owned(), ContainerState::Running);
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    let opts = LoadOptions {
        model: Some("gpt-6-luna".to_owned()),
        effort: Some(jackin_core::ReasoningEffort::Max),
        restore_container_base: Some(container_name.to_owned()),
        ..LoadOptions::default()
    };

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(temp.path()),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("cannot apply"),
        "unexpected error: {error:#}"
    );
    assert!(
        !runner.recorded.iter().any(|call| {
            call.contains("docker run")
                || call.contains("docker exec")
                || call.contains("docker start")
        }),
        "existing container must not launch or attach with a new model; recorded:\n{}",
        runner.recorded.join("\n")
    );
}
