// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_refresh_background_reuses_valid_local_image_and_skips_build_work() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
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
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    runner.fail_on = vec![
        "docker build ".to_owned(),
        "gh auth token".to_owned(),
        "docker run --rm --entrypoint".to_owned(),
        "agent_binary".to_owned(),
    ];

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("docker build "),
        "refresh-background decision must skip docker build; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("gh auth token"),
        "reuse decision must skip GitHub token lookup; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("docker run --rm --entrypoint"),
        "reuse decision must skip foreground version probe; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("agent_binary_resolve_started"),
        "reuse decision must skip runtime binary preparation; recorded:\n{recorded}"
    );

    let docker_recorded = docker.recorded.borrow();
    assert!(
        !docker_recorded
            .iter()
            .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
        "reuse decision must not check published image freshness in the foreground: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call == &format!("docker inspect image:{image}")),
        "reuse decision must inspect valid local recipe labels: {docker_recorded:?}"
    );
}

#[tokio::test]
async fn valid_image_decision_runs_before_operator_env_resolution() {
    struct FailingOpRunner;

    impl jackin_env::OpRunner for FailingOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("operator env read intentionally failed")
        }

        fn probe(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.env.insert(
        "OPERATOR_IMAGE_ORDER".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".to_owned(),
            path: "Vault/Item/Field".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    persist_test_config(&paths, &config);
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        op_runner: Some(Box::new(FailingOpRunner)),
        ..LoadOptions::default()
    };

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("operator env resolution failed"),
        "expected operator env failure after image decision, got {error:#}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker inspect image:{image}")),
        "valid image must be inspected before operator env can fail"
    );
}

#[tokio::test]
async fn stale_agent_version_cache_does_not_force_foreground_update_probe() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
    jackin_image::version_check::store_cache_bust(&paths, &image, "stored-bust");
    jackin_image::version_check::store_version(&paths, agent, &image, "1.0.0");
    let latest = jackin_image::agent_binary::AgentRelease {
        agent,
        version: "2.0.0".to_owned(),
        url: "https://example.invalid/claude".to_owned(),
        checksum: None,
        archive_member: None,
    };
    let latest_path = paths
        .cache_dir
        .join("agent-binaries")
        .join(agent.slug())
        .join("latest.json");
    std::fs::create_dir_all(latest_path.parent().unwrap()).unwrap();
    std::fs::write(latest_path, serde_json::to_string(&latest).unwrap()).unwrap();
    let stale_labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("oldsha"),
        None,
        None,
        "stored-bust",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(stale_labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker build ") && call.contains("DerivedDockerfile"))
        .expect("stale role SHA must trigger a derived image rebuild");
    assert!(
        build_cmd.contains("--build-arg JACKIN_CACHE_BUST=stored-bust"),
        "normal rebuild path must not run latest-release update probe and mint a fresh cache bust; got: {build_cmd}"
    );
}

#[tokio::test]
async fn load_agent_cleans_up_when_parallel_sidecar_start_fails() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
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
    let mut docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::Running,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker.fail_with = vec![(
        "create_container:".to_owned(),
        "dind create failed".to_owned(),
    )];
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("dind create failed"),
        "unexpected error: {error:#}"
    );
    let docker_recorded = docker.recorded.borrow();
    assert!(
        !docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && !call.ends_with("-dind")),
        "role cleanup must fail closed without a captured role ID: {docker_recorded:?}"
    );
    assert!(
        !docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && call.ends_with("-dind")),
        "DinD cleanup must fail closed without a captured DinD ID: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker volume rm jk-")),
        "cert volume cleanup missing after sidecar failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker network rm jk-")),
        "network cleanup missing after sidecar failure: {docker_recorded:?}"
    );
}
