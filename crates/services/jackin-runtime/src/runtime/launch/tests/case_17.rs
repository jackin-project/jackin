// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_starts_stopped_current_instance_before_credentials_and_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
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
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::Running);
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: container_name.to_owned(),
        dind_container_id: manifest.docker.dind_container.clone(),
    });
    write_indexed_manifest(&paths, &manifest);
    provision_restore_account_policy(&paths, &config, &manifest);
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
            ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
            ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
            ContainerState::Running,
            ContainerState::Running,
        ])),
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            String::new(),
            "Sessions: 1\n".to_owned(),
        ])),
        inspect_network_queue: std::cell::RefCell::new(VecDeque::from([Some(
            jackin_docker::docker_client::NetworkRow {
                name: manifest.docker.network.clone(),
                labels: std::collections::HashMap::default(),
            },
        )])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);
    let mut workspace = repo_workspace(&cached_repo.repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.name = "workspace".to_owned();
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

    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call == &format!("start_container:{container_name}")),
        "stopped current-role instance must be started; recorded: {docker_recorded:?}"
    );
    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker exec")
            && recorded.contains(container_name)
            && recorded.contains("jackin-capsule"),
        "started current-role instance must attach through Capsule; recorded:\n{recorded}"
    );
    for forbidden in [
        "docker build ",
        "gh auth token",
        "docker inspect image:",
        "docker run --rm --entrypoint",
    ] {
        assert!(
            !recorded.contains(forbidden),
            "stopped restore path must skip {forbidden}; recorded:\n{recorded}"
        );
    }
    assert!(
        !recorded.contains(&cached_repo.repo_dir.display().to_string()),
        "stopped restore path must not touch the cached role repo before hardline; recorded:\n{recorded}"
    );
}

#[tokio::test]
async fn load_agent_recreates_missing_current_instance_from_valid_image_without_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            mounts: repo_workspace(&cached_repo.repo_dir).mounts,
            accounts: vec!["test".to_owned()],
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(container_name, "agent-smith", "Agent Smith", agent);
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);
    provision_restore_account_policy(&paths, &config, &manifest);
    let image = crate::runtime::naming::image_name(&selector, None);
    let local_base = local_role_base_for_test(&selector, Some("abc123"));
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        Some(local_base.as_str()),
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
        ])),
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![image.clone()]])),
        inspect_image_labels_queue: std::cell::RefCell::new(VecDeque::from([labels])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let mut workspace = repo_workspace(&cached_repo.repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.name = "workspace".to_owned();

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
        recorded.contains("docker run -d")
            && recorded.contains(&format!("--name {container_name}"))
            && recorded.contains(&image),
        "valid-image recreate path must run the missing role container from the reusable image; recorded:\n{recorded}"
    );
    for forbidden in [
        "docker build ",
        "gh auth token",
        "docker run --rm --entrypoint",
    ] {
        assert!(
            !recorded.contains(forbidden),
            "valid-image recreate path must skip {forbidden}; recorded:\n{recorded}"
        );
    }
}

#[tokio::test]
async fn load_agent_passes_pull_flag_when_rebuild() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions {
            rebuild: true,
            ..LoadOptions::default()
        },
    )
    .await
    .unwrap();

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker build "))
        .unwrap();
    assert!(
        build_cmd.contains("--pull"),
        "--rebuild must pass --pull to refresh the base image"
    );
}

#[tokio::test]
async fn load_agent_rebuild_does_not_attach_running_current_instance() {
    // Regression for the `--rebuild` fast-path bypass: the early restore gate
    // is guarded by `!opts.rebuild`, but a forced rebuild then falls through to
    // the *second* restore resolution. Without the matching guard there,
    // `resolve_restore_candidate` returns `AttachCurrentRole` for a running
    // current-role container and `return`s into it — silently skipping the
    // build the operator asked for. A running current-role container is seeded
    // here (inspect queue returns `Running`) so that if the guard regresses the
    // launch attaches and records no `docker build`, failing this test.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            accounts: vec!["test".to_owned()],
            workdir: "/workspace".to_owned(),
            mounts: repo_workspace(&repo_dir).mounts,
            default_agent: Some(jackin_core::Agent::Claude),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    persist_test_config(&paths, &config);
    let mut runner = FakeRunner::for_load_agent([String::new()]);

    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    // Index a running current-role container that the resolver would attach to.
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
        ])),
        ..Default::default()
    };
    let mut workspace = repo_workspace(&repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.default_agent = Some(jackin_core::Agent::Claude);
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions {
            rebuild: true,
            ..LoadOptions::default()
        },
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker build "),
        "--rebuild must build even when a running current-role container exists \
         (must not take the attach/start fast path); recorded:\n{recorded}"
    );
}
