// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_skips_unused_github_env_resolution() {
    struct FailingUnusedGithubOpRunner;

    impl jackin_env::OpRunner for FailingUnusedGithubOpRunner {
        fn read(&self, reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("unused github env ref should not be resolved: {reference}")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let mut github_env = std::collections::BTreeMap::new();
    github_env.insert(
        jackin_core::GH_TOKEN_ENV_NAME.to_owned(),
        jackin_core::EnvValue::Plain("ghp_test".to_owned()),
    );
    github_env.insert(
        "UNUSED_GITHUB_SECRET".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/github/unused".to_owned(),
            path: "Vault/GitHub/unused".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    config.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Token,
        env: github_env,
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
    let observed_env = observe_host_env_file(&mut runner, &paths);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingUnusedGithubOpRunner)),
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

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("--env-file") && !run_cmd.contains("ghp_test"),
        "required GitHub token must use the host env file; got: {run_cmd}"
    );
    assert!(
        !run_cmd.contains("UNUSED_GITHUB_SECRET"),
        "unused GitHub env keys are not runtime env; got: {run_cmd}"
    );
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "GH_TOKEN=ghp_test")
    );
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "GITHUB_TOKEN=ghp_test")
    );
    assert!(!observed.contents.contains("UNUSED_GITHUB_SECRET"));
    assert!(!observed.path.exists());
}

#[tokio::test]
async fn load_agent_rebuild_token_preflight_failure_tears_down_adopted_dind() {
    // Regression for the adopted-prewarm-DinD leak: `adopt_prewarmed_dind_sidecar`
    // takes over a *running* prewarmed DinD container/network/volume and deletes
    // its on-disk state, so nothing re-adopts it. A fallible preflight after
    // adoption (here Token-mode GitHub auth with no resolvable token) must tear
    // those resources down rather than orphan a live privileged container.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    // Token mode with an empty env => GH_TOKEN resolves to None =>
    // `verify_github_token_present` fails, after adoption.
    config.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Token,
        env: std::collections::BTreeMap::new(),
    });
    config.docker.grants = Some(jackin_core::DockerGrants {
        dind: Some(jackin_core::DindGrant::Privileged),
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

    // Seed a kept, running prewarmed DinD so the launch adopts it.
    let prewarm_dind = "jk-prewarm-b4-dind";
    let prewarm_net = "jk-prewarm-b4-net";
    let prewarm_certs = "jk-prewarm-b4-certs";
    write_prewarmed_dind_state(
        &paths,
        &DindSidecarPrewarm {
            dind: prewarm_dind.to_owned(),
            dind_id: "prewarm-b4-dind-id".to_owned(),
            network: prewarm_net.to_owned(),
            certs_volume: prewarm_certs.to_owned(),
            ready_ms: 12,
            kept: true,
        },
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    // Image-reuse path (no build needed to reach adoption).
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
        .insert(prewarm_dind.to_owned(), "prewarm-b4-dind-id".to_owned());
    // Adoption: pin the prewarmed dind to Running by name (the restore/claim
    // inspects that run first hit the default NotFound), and give its network
    // the prewarm labels so adoption accepts it.
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

    result.expect_err("missing Token-mode GitHub token must fail the launch");
    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker rm -f {prewarm_dind}")),
        "adopted prewarm DinD must be torn down on post-adoption failure; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker network rm {prewarm_net}")),
        "adopted prewarm network must be torn down; recorded: {recorded:?}"
    );
}

#[tokio::test]
async fn load_agent_dind_free_launch_preserves_available_prewarm() {
    // Regression for the adopted-prewarm-DinD leak: `adopt_prewarmed_dind_sidecar`
    // takes over a *running* prewarmed DinD container/network/volume and deletes
    // its on-disk state, so nothing re-adopts it. A fallible preflight after
    // adoption (here Token-mode GitHub auth with no resolvable token) must tear
    // those resources down rather than orphan a live privileged container.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    // Token mode with an empty env => GH_TOKEN resolves to None =>
    // `verify_github_token_present` fails, after adoption.
    config.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Token,
        env: std::collections::BTreeMap::new(),
    });
    config.docker.grants = Some(jackin_core::DockerGrants {
        dind: Some(jackin_core::DindGrant::None),
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

    // Seed a kept, running prewarmed DinD so the launch adopts it.
    let prewarm_dind = "jk-prewarm-b4-dind";
    let prewarm_net = "jk-prewarm-b4-net";
    let prewarm_certs = "jk-prewarm-b4-certs";
    write_prewarmed_dind_state(
        &paths,
        &DindSidecarPrewarm {
            dind: prewarm_dind.to_owned(),
            dind_id: "prewarm-b4-dind-id".to_owned(),
            network: prewarm_net.to_owned(),
            certs_volume: prewarm_certs.to_owned(),
            ready_ms: 12,
            kept: true,
        },
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    // Image-reuse path (no build needed to reach adoption).
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
        .insert(prewarm_dind.to_owned(), "prewarm-b4-dind-id".to_owned());
    // Adoption: pin the prewarmed dind to Running by name (the restore/claim
    // inspects that run first hit the default NotFound), and give its network
    // the prewarm labels so adoption accepts it.
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

    result.expect_err("missing Token-mode GitHub token must fail the launch");
    let recorded = docker.recorded.borrow();
    assert!(
        !recorded
            .iter()
            .any(|call| call == &format!("docker rm -f {prewarm_dind}")
                || call == &format!("docker network rm {prewarm_net}"))
    );
    assert!(paths.data_dir.join("prewarm-dind.json").exists());
}
