// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_rolls_back_runtime_on_attached_run_failure() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner {
        fail_on: vec!["jackin.kind=role".to_owned()],
        capture_queue: VecDeque::from(vec![
            String::new(),
            String::new(),
            String::new(),
            String::new(), // identity
            String::new(), // git pull
        ]),
        ..Default::default()
    };

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
plugins = ["code-review@claude-plugins-official"]
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
            ContainerState::Created,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("docker run -d --name jk-"));
    let container_name = launched_role_container_name(&runner);
    let dind = format!("{container_name}-dind");
    // The observation seam fails after typed create/start captured the role
    // identity. Cleanup must therefore remove the exact role and sidecar
    // identities, not rediscover either by name.
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker rm -f {container_name}"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .all(|call| call != &format!("docker rm -f {dind}")),
        "cleanup must not remove a sidecar without its captured identity"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.starts_with("docker volume rm"))
            && docker
                .recorded
                .borrow()
                .iter()
                .any(|call| call.starts_with("docker network rm")),
        "captured identities must authorize shared-resource cleanup: {:?}",
        docker.recorded.borrow()
    );
}

#[tokio::test]
async fn load_agent_checks_dind_readiness() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
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

    let workspace = repo_workspace(&repo_dir);
    let docker = fake_docker_for_clean_attached_exit();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let (dind, _) = launched_dind_container(&docker);
    // DinD readiness check polls via docker exec (bollard)
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.contains(&format!("docker exec {dind} docker info")))
    );

    // DinD container is created/started through DockerApi before readiness checks.
    let docker_recorded = docker.recorded.borrow();
    let dind_start = docker_recorded
        .iter()
        .position(|call| call == &format!("start_container:{dind}"))
        .unwrap();
    // docker exec calls go through bollard docker.exec_capture
    let dind_info = docker_recorded
        .iter()
        .position(|call| call.contains(&format!("docker exec {dind} docker info")))
        .unwrap();
    assert!(
        dind_start < dind_info,
        "DinD must start before readiness polling; recorded: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.contains(&format!("docker exec {dind} docker info"))),
        "DinD readiness docker info check must be recorded; recorded: {docker_recorded:?}"
    );

    // TLS cert verification also via docker.exec_capture
    assert!(docker_recorded.iter().any(|call| {
        call.contains(&format!("docker exec {dind} test -f /certs/client/ca.pem"))
    }));
}

#[tokio::test]
async fn load_agent_configures_dind_with_tls() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
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

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let (dind, dind_spec) = launched_dind_container(&docker);
    let certs_volume = dind.strip_suffix("-dind").unwrap().to_owned() + "-dind-certs";
    assert!(crate::instance::naming::is_dns_label(&dind), "{dind}");

    // DinD sidecar: TLS enabled with cert volume.
    assert!(
        dind_spec
            .env
            .contains(&"DOCKER_TLS_CERTDIR=/certs".to_owned()),
        "DinD must enable TLS cert generation"
    );
    assert!(
        dind_spec
            .binds
            .contains(&format!("{certs_volume}:/certs/client")),
        "DinD must mount cert volume"
    );
    // DinD's auto-generated server cert must include the container name as a
    // Subject Alternative Name, because the role connects via
    // DOCKER_HOST=tcp://{dind}:2376. Without this, the TLS
    // handshake fails because the default SANs only cover the short
    // container ID, `docker`, and `localhost`.
    //
    // The `DNS:` prefix is mandatory: `dockerd-entrypoint.sh` passes
    // `DOCKER_TLS_SAN` through to openssl verbatim (without adding a type
    // prefix), and openssl rejects SAN entries that lack a type tag with
    // `v2i_GENERAL_NAME_ex: missing value`.
    assert!(
        dind_spec
            .env
            .contains(&format!("DOCKER_TLS_SAN=DNS:{dind}")),
        "DinD SAN value must be prefixed with `DNS:` so openssl accepts it"
    );
    assert!(dind_spec.privileged, "DinD must run privileged");
    let expected_network = dind.strip_suffix("-dind").unwrap().to_owned() + "-net";
    assert_eq!(dind_spec.network, expected_network);
    assert_eq!(dind_spec.image, "docker:29-dind");

    // Role container: TLS client config
    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == format!("DOCKER_HOST=tcp://{dind}:2376")),
        "role must use TLS port 2376"
    );
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == format!("TESTCONTAINERS_HOST_OVERRIDE={dind}")),
        "Testcontainers must receive the same DNS-safe DinD hostname"
    );
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "DOCKER_TLS_VERIFY=1"),
        "role must verify TLS"
    );
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "DOCKER_CERT_PATH=/jackin/run/dind-certs/client"),
        "role must know cert path"
    );
    assert!(
        run_cmd.contains(&format!("{certs_volume}:/jackin/run/dind-certs/client:ro")),
        "role must mount cert volume read-only"
    );
    assert!(!run_cmd.contains("DOCKER_HOST="));
    assert!(!observed.path.exists());
}
