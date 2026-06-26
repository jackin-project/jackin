//! `DockerApi` trait and pure data types for container operations.
//!
//! This module contains only the trait definition and associated data types —
//! no bollard, no tokio, no Docker daemon connection. The concrete
//! `BollardDockerClient` implementation lives in the binary crate
//! (`docker_client/mod.rs`) until it migrates to `jackin-runtime`.

use std::collections::HashMap;

/// Runtime state of a container as returned by the Docker API.
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

    /// Returns `true` for every state except `NotFound`.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        !matches!(self, Self::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRow {
    pub name: String,
    pub labels: HashMap<String, String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// Async Docker daemon API seam. Dependency-injected so tests can stub Docker
/// without a running daemon.
pub trait DockerApi {
    #[must_use]
    async fn inspect_container_state(&self, name: &str) -> ContainerState;
    async fn remove_container(&self, name: &str) -> anyhow::Result<()>;
    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>>;
    async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()>;
    async fn start_container(&self, name: &str) -> anyhow::Result<()>;
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
    async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String>;
}
