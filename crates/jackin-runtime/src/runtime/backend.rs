//! Backend lifecycle dispatch for persisted runtime instances.

use anyhow::Result;
use jackin_core::CommandRunner;
use jackin_core::paths::JackinPaths;
use jackin_docker::docker_client::DockerApi;

use crate::apple_container_client::{AppleContainerApi, AppleContainerClient};
use crate::instance::{BackendResources, InstanceManifest};

/// Common lifecycle operations every persisted container backend must expose.
///
/// Launch remains backend-specific today because Docker and Apple Container
/// carry different launch inputs. Instance management is unified here so
/// teardown and reconnect paths cannot silently remain Docker-only.
pub trait ContainerBackend {
    async fn eject(&self, paths: &JackinPaths, container_name: &str) -> Result<()>;

    async fn ensure_absent_for_purge(
        &self,
        paths: &JackinPaths,
        container_name: &str,
    ) -> Result<()>;

    async fn reconnect(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        focus_session: Option<u64>,
        runner: &mut impl CommandRunner,
    ) -> Result<()>;

    async fn hardline(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        focus_session: Option<u64>,
        runner: &mut impl CommandRunner,
    ) -> Result<()>;

    async fn finalize(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        runner: &mut impl CommandRunner,
    ) -> Result<()>;
}

#[derive(Debug)]
pub struct DockerBackend<'a, D> {
    docker: &'a D,
}

impl<'a, D> DockerBackend<'a, D> {
    pub const fn new(docker: &'a D) -> Self {
        Self { docker }
    }
}

impl<D> ContainerBackend for DockerBackend<'_, D>
where
    D: DockerApi,
{
    async fn eject(&self, paths: &JackinPaths, container_name: &str) -> Result<()> {
        super::cleanup::eject_docker_role(paths, container_name, self.docker).await
    }

    async fn ensure_absent_for_purge(
        &self,
        paths: &JackinPaths,
        container_name: &str,
    ) -> Result<()> {
        let resources = super::cleanup::docker_resources_for_state(paths, container_name);
        super::cleanup::ensure_role_resources_absent_for_purge(self.docker, &resources).await
    }

    async fn reconnect(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        focus_session: Option<u64>,
        runner: &mut impl CommandRunner,
    ) -> Result<()> {
        if focus_session.is_some() {
            super::attach::reconnect_or_create_session_with_focus(
                paths,
                container_name,
                focus_session,
                self.docker,
                runner,
            )
            .await
        } else {
            super::attach::start_or_reconnect_capsule_client(
                paths,
                container_name,
                self.docker,
                runner,
            )
            .await
        }
    }

    async fn hardline(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        focus_session: Option<u64>,
        runner: &mut impl CommandRunner,
    ) -> Result<()> {
        super::attach::hardline_docker_agent_with_focus(
            paths,
            container_name,
            focus_session,
            self.docker,
            runner,
        )
        .await
    }

    async fn finalize(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        runner: &mut impl CommandRunner,
    ) -> Result<()> {
        super::attach::finalize_reconnected_foreground_session(
            paths,
            container_name,
            self.docker,
            runner,
        )
        .await
    }
}

#[derive(Debug)]
pub struct AppleContainerBackend<C = AppleContainerClient> {
    client: C,
}

impl AppleContainerBackend<AppleContainerClient> {
    pub fn production() -> Self {
        Self {
            client: AppleContainerClient::new(),
        }
    }
}

impl<C> AppleContainerBackend<C> {
    pub const fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> ContainerBackend for AppleContainerBackend<C>
where
    C: AppleContainerApi,
{
    async fn eject(&self, _paths: &JackinPaths, container_name: &str) -> Result<()> {
        crate::runtime::apple_container::stop_with(&self.client, container_name).await
    }

    async fn ensure_absent_for_purge(
        &self,
        _paths: &JackinPaths,
        container_name: &str,
    ) -> Result<()> {
        crate::runtime::apple_container::ensure_absent_for_purge_with(&self.client, container_name)
            .await
    }

    async fn reconnect(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        focus_session: Option<u64>,
        _runner: &mut impl CommandRunner,
    ) -> Result<()> {
        crate::runtime::apple_container::reconnect(paths, container_name, focus_session).await
    }

    async fn hardline(
        &self,
        paths: &JackinPaths,
        container_name: &str,
        focus_session: Option<u64>,
        runner: &mut impl CommandRunner,
    ) -> Result<()> {
        self.reconnect(paths, container_name, focus_session, runner)
            .await?;
        self.finalize(paths, container_name, runner).await
    }

    async fn finalize(
        &self,
        _paths: &JackinPaths,
        _container_name: &str,
        _runner: &mut impl CommandRunner,
    ) -> Result<()> {
        anyhow::bail!("apple-container finalize not yet implemented - Phase 0")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceBackend {
    Docker,
    AppleContainer,
}

pub fn backend_for_manifest(manifest: Option<&InstanceManifest>) -> InstanceBackend {
    match manifest.and_then(|manifest| manifest.backend.as_ref()) {
        Some(BackendResources::AppleContainer(_)) => InstanceBackend::AppleContainer,
        Some(BackendResources::Docker(_)) | None => InstanceBackend::Docker,
    }
}

pub fn backend_for_state(paths: &JackinPaths, container_name: &str) -> InstanceBackend {
    let state_dir = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read_or_log(&state_dir, "backend_for_state");
    backend_for_manifest(manifest.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apple_container_client::{AppleContainerInfo, FakeAppleContainerClient};
    use crate::instance::{AppleContainerResources, DockerResources, NewInstanceManifest};
    use crate::runtime::test_support::FakeRunner;
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
}
