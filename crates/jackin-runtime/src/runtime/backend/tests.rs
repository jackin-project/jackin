use super::*;
use crate::apple_container_client::{AppleContainerInfo, FakeAppleContainerClient};
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
        agent_runtime: jackin_core::agent::Agent::Claude,
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
        name: "jk-agent-smith".to_owned(),
        status: "running".to_owned(),
    });
    let backend = AppleContainerBackend::new(client);

    backend.eject(&paths, "jk-agent-smith").await.unwrap();
    let err = backend
        .ensure_absent_for_purge(&paths, "jk-agent-smith")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("still exists"), "got: {err}");
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
