use super::*;
use crate::apple_container_client::{
    AppleContainerInfo, AppleContainerStatus, FakeAppleContainerClient,
};
use crate::instance::{AppleContainerResources, DockerResources, NewInstanceManifest};
use jackin_test_support::FakeRunner;
use tempfile::tempdir;

fn test_manifest(container: &str, backend: Option<BackendResources>) -> InstanceManifest {
    let input = NewInstanceManifest {
        container_base: container,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources::from_container_name(container),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    };
    if let Some(resources) = backend {
        InstanceManifest::new_with_backend(input, resources)
    } else {
        InstanceManifest::new(input)
    }
}

fn provision_empty_account_admission(paths: &JackinPaths, container: &str) {
    let config = jackin_config::AppConfig::default();
    let workspace = jackin_config::WorkspaceConfig {
        workdir: "/workspace".to_owned(),
        mounts: vec![jackin_config::MountConfig {
            src: paths.config_dir.display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        ..jackin_config::WorkspaceConfig::default()
    };
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.config_dir.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    std::fs::write(
        paths.workspaces_dir.join("workspace.toml"),
        toml::to_string(&workspace).unwrap(),
    )
    .unwrap();
    std::fs::File::create(paths.config_file.with_file_name("config.lock")).unwrap();
    let snapshot = jackin_config::load_read_only_config_snapshot(paths).unwrap();
    assert!(
        snapshot.diagnostics.is_empty(),
        "invalid admission fixture: {:?}",
        snapshot.diagnostics
    );
    assert_eq!(
        snapshot.config.workspaces["workspace"].workdir, "/workspace",
        "admission fixture must load the configured workspace used by its manifest"
    );

    let root = paths.data_dir.join(container);
    std::fs::create_dir_all(&root).unwrap();
    let manifest = test_manifest(container, None);
    manifest.write(&root).unwrap();
    let workspace = manifest
        .workspace_name
        .as_deref()
        .map(jackin_core::WorkspaceName::parse)
        .transpose()
        .unwrap();
    let digest = crate::runtime::launch::account_configuration_fingerprint(
        &snapshot.config,
        workspace.as_ref(),
        &manifest.role_key,
        &manifest.admitted_instances,
    )
    .unwrap();
    std::fs::write(root.join("account-admission.sha256"), digest).unwrap();
}

#[test]
fn backend_for_manifest_defaults_legacy_instances_to_docker() {
    let manifest = test_manifest("jk-agent-smith", None);

    assert_eq!(
        backend_for_manifest(Some(&manifest)),
        InstanceBackend::Docker
    );
    assert_eq!(backend_for_manifest(None), InstanceBackend::Docker);
}

#[test]
fn backend_for_manifest_reads_apple_container_resources() {
    let manifest = test_manifest(
        "jk-agent-smith",
        Some(BackendResources::AppleContainer(AppleContainerResources {
            container_name: "jk-agent-smith".to_owned(),
            role_image_ref: "jk_agent-smith".to_owned(),
            inner_docker_enabled: false,
        })),
    );

    assert_eq!(
        backend_for_manifest(Some(&manifest)),
        InstanceBackend::AppleContainer
    );
}

#[tokio::test]
async fn apple_backend_eject_and_purge_use_apple_client() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let client = FakeAppleContainerClient::new();
    client.containers.lock().unwrap().push(AppleContainerInfo {
        id: "jk-agent-smith".to_owned(),
        status: AppleContainerStatus::Running,
    });
    let backend = AppleContainerBackend::new(client);

    backend.eject(&paths, "jk-agent-smith").await.unwrap();
    let after_eject = backend
        .client
        .inspect_container("jk-agent-smith")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_eject.status, AppleContainerStatus::Stopped);
    let err = backend
        .ensure_absent_for_purge(&paths, "jk-agent-smith")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("still exists"), "got: {err}");
}

#[tokio::test]
async fn apple_backend_reconnect_uses_injected_client_after_admission_and_abstains_on_unknown() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let name = "jk-agent-smith";
    provision_empty_account_admission(&paths, name);
    let client = FakeAppleContainerClient::new();
    client.containers.lock().unwrap().push(AppleContainerInfo {
        id: name.to_owned(),
        status: AppleContainerStatus::Unknown,
    });
    let backend = AppleContainerBackend::new(client);
    let mut runner = FakeRunner::default();

    let error = backend
        .reconnect(&paths, name, None, &mut runner)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("uncertain lifecycle state"));
    assert_eq!(
        *backend.client.inspect_calls.lock().unwrap(),
        vec![name.to_owned()]
    );
    assert!(backend.client.start_calls.lock().unwrap().is_empty());
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn apple_backend_reconnect_keeps_account_admission_as_a_precondition() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let name = "jk-agent-smith";
    let client = FakeAppleContainerClient::new();
    client.containers.lock().unwrap().push(AppleContainerInfo {
        id: name.to_owned(),
        status: AppleContainerStatus::Running,
    });
    let backend = AppleContainerBackend::new(client);
    let mut runner = FakeRunner::default();

    let result = backend.reconnect(&paths, name, None, &mut runner).await;

    assert!(result.is_err());
    assert!(backend.client.inspect_calls.lock().unwrap().is_empty());
    assert!(backend.client.start_calls.lock().unwrap().is_empty());
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn apple_backend_finalize_is_explicit_phase0_error() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let backend = AppleContainerBackend::new(FakeAppleContainerClient::new());
    let mut runner = FakeRunner::default();

    let err = backend
        .finalize(&paths, "jk-agent-smith", &mut runner)
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("apple-container finalize not yet implemented - Phase 0"),
        "got: {err}"
    );
}
