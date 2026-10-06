// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[tokio::test]
async fn load_agent_injects_op_cli_resolved_value() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    let bin_dir = temp.path().join("fake-bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin_path = bin_dir.join("op");
    // The resolver first runs `op --version` as a reachability probe
    // when any value carries an OpRef, then calls `op read -- op://...`
    // with the canonical UUID URI. The fake must handle both.
    std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.30.0'; exit 0; fi\nif [ \"$1\" = \"read\" ]; then\n  for arg in \"$@\"; do\n    if [ \"$arg\" = \"op://abc-vault/abc-item/api-token\" ]; then printf '%s' 'resolved-op-token'; exit 0; fi\n  done\nfi\nexit 99\n",
        )
        .unwrap();
    let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&bin_path, perms).unwrap();

    std::fs::write(
        &paths.config_file,
        format!(
            "{SINGLETON_CLAUDE_TOML}{}",
            r#"[env]
OPERATOR_TOKEN = {op = "op://abc-vault/abc-item/api-token", path = "Personal/api/token"}

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#
        ),
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
    let observed_env = observe_host_env_file(&mut runner, &paths);

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

    // Inject the fake `op` binary path via `LoadOptions::op_runner`.
    // No process env mutation — `OpCli::with_binary` takes the path
    // as a direct argument, so the `unsafe_code = "forbid"`
    // crate-level lint stays intact and sibling tests running in
    // parallel via cargo-nextest cannot race on any shared env var.
    let op_runner: Box<dyn jackin_env::OpRunner> = Box::new(jackin_env::OpCli::with_binary(
        bin_path.to_string_lossy().to_string(),
    ));
    let opts = LoadOptions {
        op_runner: Some(op_runner),
        ..LoadOptions::default()
    };

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
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
        run_cmd.contains("--env-file") && !run_cmd.contains("resolved-op-token"),
        "op:// ref must resolve without entering argv; got: {run_cmd}"
    );
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "OPERATOR_TOKEN=resolved-op-token")
    );
    assert!(!observed.path.exists());
}

#[tokio::test]
async fn claim_container_name_not_found_claims_unique_ad_hoc_name() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // inspect returns NotFound (empty queue)
    let docker = jackin_test_support::FakeDockerClient::default();
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.contains("agentsmith"), "{name}");
    assert!(!name.contains("clone"), "{name}");
    assert!(crate::instance::naming::is_dns_label(&name), "{name}");
    assert!(
        crate::instance::naming::is_dns_label(&format!("{name}-dind")),
        "{name}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.contains("docker inspect"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.contains("docker rm"))
    );
}

#[tokio::test]
async fn claim_container_name_docker_unavailable_errors() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    let docker = jackin_test_support::FakeDockerClient {
        fail_with: vec![(
            "docker inspect".to_owned(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
        )],
        ..Default::default()
    };
    let err = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("cannot claim container name"));
    assert!(err.to_string().contains("Docker is unavailable"));
}

#[tokio::test]
async fn claim_container_name_running_collision_tries_another_unique_name() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // First inspect → Running (occupied), second → NotFound (claimed)
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.ends_with("-agentsmith"), "{name}");
    assert!(!name.contains("clone"), "{name}");
    assert_eq!(
        docker
            .recorded
            .borrow()
            .iter()
            .filter(|c| c.contains("docker inspect"))
            .count(),
        2
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm"))
    );
}

#[tokio::test]
async fn claim_container_name_clean_exit_removes_and_reclaims() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // Stopped with exit_code=0, oom_killed=false → remove and reclaim
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.ends_with("-agentsmith"), "{name}");
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f") && c.contains("agentsmith"))
    );
}

#[tokio::test]
async fn claim_container_name_crashed_collision_tries_another_unique_name() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // Stopped with exit_code=1 → skip (no rm), then NotFound → claim
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Stopped {
                exit_code: 1,
                oom_killed: false,
            },
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.ends_with("-agentsmith"), "{name}");
    assert!(!name.contains("clone"), "{name}");
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm"))
    );
}

#[tokio::test]
async fn claim_container_name_saved_workspace_includes_workspace_component() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    let docker = jackin_test_support::FakeDockerClient::default();
    let (name, _lock) = claim_container_name(
        &paths,
        Some(&WorkspaceName::parse("my-workspace").unwrap()),
        &selector,
        &docker,
    )
    .await
    .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(
        name.contains("myworkspace") && name.ends_with("-agentsmith"),
        "{name}"
    );
    assert!(name.len() <= 58, "{name}");
}

#[tokio::test]
async fn missing_matching_instance_recreates_current_role() {
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
    manifest
        .write(&paths.data_dir.join(container_name))
        .unwrap();
    // Missing current-role containers can be recreated in-place. The image
    // decision later decides whether that recreate can reuse the local image
    // or must rebuild.
    let docker = jackin_test_support::FakeDockerClient::default();

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(
        candidate,
        RestoreResolution::RecreateCurrentRole(container_name.to_owned())
    );
}

#[tokio::test]
async fn running_matching_instance_is_skipped_by_launch_path() {
    // D13: launch never reconnects to a live instance. Running container →
    // StartFresh (let launch proceed to create a new container).
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
async fn stopped_matching_instance_starts_current_role() {
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
    // Stopped current-role containers can be started and reconnected without
    // rebuilding or resolving launch credentials, as long as network exists.
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        inspect_network_queue: std::cell::RefCell::new(VecDeque::from([Some(
            jackin_docker::docker_client::NetworkRow {
                name: manifest.docker.network.clone(),
                labels: std::collections::HashMap::default(),
            },
        )])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(
        candidate,
        RestoreResolution::StartCurrentRoleWithHandle(
            jackin_core::ContainerHandle::new(container_name, container_name).unwrap(),
        )
    );
}
