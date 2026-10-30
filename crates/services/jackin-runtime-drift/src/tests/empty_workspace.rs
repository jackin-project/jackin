// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_core::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, ContainerState, DockerApi,
    JackinPaths, NetworkRow, RemoveImageOutcome, WorkspaceName,
};

#[derive(Debug)]
struct EmptyDocker;

impl DockerApi for EmptyDocker {
    async fn ping(&self) -> anyhow::Result<()> {
        std::future::ready(()).await;
        Ok(())
    }

    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection {
        std::future::ready(()).await;
        let _ = name;
        ContainerInspection {
            handle: None,
            state: ContainerState::NotFound,
        }
    }

    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState {
        std::future::ready(()).await;
        let _ = container;
        ContainerState::NotFound
    }

    async fn remove_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        let _ = container;
        anyhow::bail!("stub docker has no containers");
    }

    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        let _ = (label_filters, all);
        anyhow::bail!("stub docker must not be consulted for an empty workspace");
    }

    async fn create_container(
        &self,
        name: &str,
        spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle> {
        let (_name, _spec) = (name, spec);
        anyhow::bail!("stub docker cannot create containers");
    }

    async fn start_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        let _ = container;
        anyhow::bail!("stub docker cannot start containers");
    }

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
        let _ = name;
        anyhow::bail!("stub docker cannot remove volumes");
    }

    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<()> {
        let (_name, _labels, _internal) = (name, labels, internal);
        anyhow::bail!("stub docker cannot create networks");
    }

    async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
        let _ = name;
        anyhow::bail!("stub docker cannot remove networks");
    }

    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        let _ = label_filters;
        anyhow::bail!("stub docker cannot list networks");
    }

    async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
        let _ = name;
        anyhow::bail!("stub docker cannot inspect networks");
    }

    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>> {
        let _ = reference_filter;
        anyhow::bail!("stub docker cannot list images");
    }

    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome> {
        let _ = name;
        anyhow::bail!("stub docker cannot remove images");
    }

    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>> {
        let _ = image;
        anyhow::bail!("stub docker cannot inspect images");
    }

    async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
        let _ = image;
        anyhow::bail!("stub docker cannot pull images");
    }

    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String> {
        let _ = (container, cmd);
        anyhow::bail!("stub docker cannot exec");
    }
}

fn unique_root(case: &str) -> anyhow::Result<std::path::PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "jackin-drift-{case}-{pid}-{id}",
        pid = std::process::id()
    ));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

#[tokio::test]
async fn empty_workspace_reports_no_drift_without_docker() -> anyhow::Result<()> {
    let root = unique_root("empty")?;
    let paths = JackinPaths::for_tests(&root);
    let workspace = WorkspaceName::parse("drift-probe")?;
    let docker = EmptyDocker;
    let detection =
        crate::drift::detect_workspace_edit_drift(&paths, &workspace, &[], &docker).await?;
    assert!(detection.running_containers.is_empty());
    assert!(detection.stopped_records.is_empty());
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[tokio::test]
async fn edited_mounts_are_ignored_for_empty_workspace() -> anyhow::Result<()> {
    let root = unique_root("edited")?;
    let paths = JackinPaths::for_tests(&root);
    let workspace = WorkspaceName::parse("drift-probe")?;
    let edited = [jackin_config::MountConfig {
        src: "/workspace".to_owned(),
        dst: "/work".to_owned(),
        readonly: false,
        isolation: jackin_core::MountIsolation::default(),
    }];
    let docker = EmptyDocker;
    let detection =
        crate::drift::detect_workspace_edit_drift(&paths, &workspace, &edited, &docker).await?;
    assert!(detection.running_containers.is_empty());
    assert!(detection.stopped_records.is_empty());
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
