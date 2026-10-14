// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerState {
    NotFound,
    InspectUnavailable(String),
    Running,
    Paused,
    Restarting,
    Removing,
    Created,
    Dead,

    Stopped { exit_code: i32, oom_killed: bool },
}

impl ContainerState {
    #[must_use]
    pub fn short_label(&self) -> String {
        match self {
            Self::Running => "running".to_owned(),
            Self::Paused => "paused".to_owned(),
            Self::Restarting => "restarting".to_owned(),
            Self::Removing => "removing".to_owned(),
            Self::Created => "created".to_owned(),
            Self::Dead => "dead".to_owned(),
            Self::Stopped {
                exit_code,
                oom_killed: false,
            } => format!("stopped exit:{exit_code}"),
            Self::Stopped {
                oom_killed: true, ..
            } => "stopped oom_killed".to_owned(),
            Self::NotFound => "missing".to_owned(),
            Self::InspectUnavailable(_) => "unavailable".to_owned(),
        }
    }

    #[must_use]
    pub fn inspect_label(&self) -> String {
        match self {
            Self::InspectUnavailable(reason) => format!("unavailable: {reason}"),
            _ => self.short_label(),
        }
    }

    #[must_use]
    pub const fn is_present(&self) -> bool {
        !matches!(self, Self::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHandle {
    name: String,
    id: String,
}

impl ContainerHandle {
    pub fn new(name: impl Into<String>, id: impl Into<String>) -> anyhow::Result<Self> {
        let name = name.into();
        let id = id.into();
        anyhow::ensure!(!name.is_empty(), "Docker container name is empty");
        anyhow::ensure!(!id.is_empty(), "Docker container ID is empty");
        Ok(Self { name, id })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInspection {
    pub handle: Option<ContainerHandle>,
    pub state: ContainerState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRow {
    pub name: String,
    pub id: String,
    pub labels: HashMap<String, String>,
}

impl ContainerRow {
    pub fn handle(&self) -> anyhow::Result<ContainerHandle> {
        ContainerHandle::new(self.name.clone(), self.id.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRow {
    pub name: String,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveImageOutcome {
    Removed,
    InUse,
    NotFound,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContainerSpec {
    pub image: String,
    pub hostname: Option<String>,
    pub env: Vec<String>,
    pub labels: HashMap<String, String>,
    pub network: String,
    pub binds: Vec<String>,
    pub entrypoint: Option<Vec<String>>,
    pub privileged: bool,
    pub workdir: Option<String>,
    pub user: Option<String>,
    pub command: Option<Vec<String>>,
    pub cap_add: Vec<String>,
    pub cap_drop: Vec<String>,
    pub readonly_rootfs: bool,
    pub security_opt: Vec<String>,
    pub tmpfs: Vec<String>,
    pub extra_hosts: Vec<String>,
    pub memory_bytes: Option<u64>,
    pub memory_reservation_bytes: Option<u64>,
    pub nano_cpus: Option<i64>,
    pub pids_limit: Option<i64>,
    pub nofile: Option<u64>,
}

pub trait DockerApi {
    async fn ping(&self) -> anyhow::Result<()>;
    #[must_use]
    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection;
    #[must_use]
    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState;
    async fn remove_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()>;
    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>>;
    async fn create_container(
        &self,
        name: &str,
        spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle>;
    async fn start_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()>;
    async fn remove_volume(&self, name: &str) -> anyhow::Result<()>;
    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<()>;
    async fn remove_network(&self, name: &str) -> anyhow::Result<()>;
    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>>;
    async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>>;
    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>>;
    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome>;
    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>>;
    async fn inspect_image_label(
        &self,
        image: &str,
        label: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(self.inspect_image_labels(image).await?.remove(label))
    }
    async fn pull_image(&self, image: &str) -> anyhow::Result<()>;
    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String>;
}
