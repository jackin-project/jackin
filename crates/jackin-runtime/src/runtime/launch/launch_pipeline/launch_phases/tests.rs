//! Suite A: grant-failure ordering + mid-pipeline FailedSetup cleanup.
use super::*;
use crate::instance::{DockerResources, InstanceManifest, NewInstanceManifest};
use jackin_config::AppConfig;
use jackin_core::agent::Agent;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_test_support::FakeDockerClient;
use std::collections::VecDeque;
use tempfile::tempdir;

fn test_manifest(container: &str) -> InstanceManifest {
    let role_source_git = "https://example.invalid/agent-smith.git";
    InstanceManifest::new(NewInstanceManifest {
        container_base: container,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: Agent::Claude,
        role_source_git,
        role_source_ref: None,
        image_tag: "projectjackin/agent-smith:test",
        docker: DockerResources::from_container_name(container),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

#[test]
fn grant_phase_rejects_root_sudo_without_docker_io() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.docker.grants = Some(DockerGrants {
        user: Some("root".to_owned()),
        sudo: Some(true),
        ..Default::default()
    });
    let selector = RoleSelector::new(None, "agent-smith");
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        "version = \"v1alpha4\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\n",
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let role_manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let err = validate_launch_grants(GrantPhaseInput {
        config: &config,
        workspace_label: "workspace",
        workspace_docker: None,
        opts_docker_profile: None,
        selector: &selector,
        role_manifest: &role_manifest,
    })
    .unwrap_err();
    assert!(
        err.to_string().contains("docker grants validation failed"),
        "{err}"
    );
}

#[tokio::test]
async fn grant_failure_cleanup_removes_adopted_sidecar_resources() {
    let docker = FakeDockerClient::default();
    let cleanup = LoadCleanup::new(
        "jk-role".into(),
        "jk-role-dind".into(),
        "jk-role-certs".into(),
        "jk-role-net".into(),
        std::env::temp_dir().join("jackin-suite-a-sock"),
    );
    cleanup_after_grant_failure(&cleanup, &docker).await;
    let recorded = docker.recorded.borrow();
    assert!(
        recorded.iter().any(|c| c == "docker rm -f jk-role-dind"),
        "grant-failure cleanup must remove DinD; recorded: {recorded:?}"
    );
    assert!(
        recorded.iter().any(|c| c == "docker network rm jk-role-net"),
        "grant-failure cleanup must remove network; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|c| c == "docker volume rm jk-role-certs"),
        "grant-failure cleanup must remove certs volume; recorded: {recorded:?}"
    );
}

#[tokio::test]
async fn mid_pipeline_failed_setup_still_runs_cleanup() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container = "jk-failed-setup-suite-a";
    let container_state = paths.data_dir.join(container);
    std::fs::create_dir_all(&container_state).unwrap();
    let mut manifest = test_manifest(container);
    manifest.write(&container_state).unwrap();

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::new()),
        ..Default::default()
    };
    let cleanup = LoadCleanup::new(
        container.into(),
        format!("{container}-dind"),
        format!("{container}-certs"),
        format!("{container}-net"),
        paths.jackin_home.join("sockets").join(container),
    );

    mark_failed_setup_then_cleanup(
        &paths,
        &container_state,
        container,
        &mut manifest,
        &cleanup,
        &docker,
        "workspace materialization",
    )
    .await;

    let reloaded = InstanceManifest::read(&container_state).unwrap();
    assert_eq!(
        reloaded.status,
        InstanceStatus::FailedSetup,
        "mid-pipeline failure must stamp FailedSetup"
    );
    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|c| c == &format!("docker rm -f {container}-dind")),
        "FailedSetup path must still tear down DinD; recorded: {recorded:?}"
    );
}

#[test]
fn typestate_grants_validated_carries_dind_flag() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        "version = \"v1alpha4\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\n",
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let role_manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let validated = validate_launch_grants(GrantPhaseInput {
        config: &config,
        workspace_label: "workspace",
        workspace_docker: None,
        opts_docker_profile: None,
        selector: &selector,
        role_manifest: &role_manifest,
    })
    .expect("default grants valid");
    let _profile = format!("{:?}", validated.resolved_profile);
    let _source = format!("{}", validated.profile_source);
    let _dind = validated.dind_started;
    drop(validated.effective_grants);
}
