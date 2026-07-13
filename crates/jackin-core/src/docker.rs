// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    /// No container with that name exists.
    NotFound,
    /// Inspect failed for a reason other than missing (daemon error, etc.).
    InspectUnavailable(String),
    /// Container process is running.
    Running,
    /// Container is paused.
    Paused,
    /// Container is restarting.
    Restarting,
    /// Container is being removed.
    Removing,
    /// Container was created but never started (or not yet started).
    Created,
    /// Container is in the dead state.
    Dead,
    /// Container has exited.
    Stopped {
        /// Process exit code from the last run.
        exit_code: i32,
        /// Whether the kernel OOM-killed the container.
        oom_killed: bool,
    },
}

impl ContainerState {
    /// Short operator-facing status label (no inspect-failure detail).
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

    /// Status label including inspect-failure reason when present.
    #[must_use]
    pub fn inspect_label(&self) -> String {
        match self {
            Self::InspectUnavailable(reason) => format!("unavailable: {reason}"),
            _ => self.short_label(),
        }
    }

    /// Returns `true` for every state except [`ContainerState::NotFound`].
    #[must_use]
    pub const fn is_present(&self) -> bool {
        !matches!(self, Self::NotFound)
    }
}

/// One container row from a list/filter query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRow {
    /// Docker container name (without leading `/` when normalized).
    pub name: String,
    /// Container labels as returned by the daemon.
    pub labels: HashMap<String, String>,
}

/// One network row from a list/filter query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRow {
    /// Docker network name.
    pub name: String,
    /// Network labels as returned by the daemon.
    pub labels: HashMap<String, String>,
}

/// Result of attempting to delete an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveImageOutcome {
    /// Image was deleted.
    Removed,
    /// Image is still referenced by a container.
    InUse,
    /// Image did not exist.
    NotFound,
}

/// Create-container parameters used by launch paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSpec {
    /// Image reference to run.
    pub image: String,
    /// Optional container hostname.
    pub hostname: Option<String>,
    /// Environment entries as `KEY=VALUE` strings.
    pub env: Vec<String>,
    /// Labels applied at create time.
    pub labels: HashMap<String, String>,
    /// Network name to attach.
    pub network: String,
    /// Bind mounts as Docker bind strings.
    pub binds: Vec<String>,
    /// Optional entrypoint override.
    pub entrypoint: Option<Vec<String>>,
    /// Whether to start the container privileged.
    pub privileged: bool,
    /// Optional working directory inside the container.
    pub workdir: Option<String>,
}

/// Async Docker daemon API seam. Dependency-injected so tests can stub Docker
/// without a running daemon.
pub trait DockerApi {
    /// Ping the daemon (`/_ping`).
    async fn ping(&self) -> anyhow::Result<()>;
    /// Inspect a container by name into a [`ContainerState`].
    #[must_use]
    async fn inspect_container_state(&self, name: &str) -> ContainerState;
    /// Force-remove a container by name.
    async fn remove_container(&self, name: &str) -> anyhow::Result<()>;
    /// List containers matching label filters; `all` includes stopped ones.
    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>>;
    /// Create a container with `name` from `spec` (does not start it).
    async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()>;
    /// Start a previously created container.
    async fn start_container(&self, name: &str) -> anyhow::Result<()>;
    /// Remove a named volume.
    async fn remove_volume(&self, name: &str) -> anyhow::Result<()>;
    /// Create a network with optional labels; `internal` isolates it from the host.
    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<()>;
    /// Remove a network by name.
    async fn remove_network(&self, name: &str) -> anyhow::Result<()>;
    /// List networks matching label filters.
    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>>;
    /// Inspect a network by name; `None` when missing.
    async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>>;
    /// List local image tags matching a reference filter.
    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>>;
    /// Remove an image by name/tag/id.
    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome>;
    /// Return all labels on an image.
    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>>;
    /// Return a single image label value, if set.
    async fn inspect_image_label(
        &self,
        image: &str,
        label: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(self.inspect_image_labels(image).await?.remove(label))
    }
    /// Pull an image reference from a registry.
    async fn pull_image(&self, image: &str) -> anyhow::Result<()>;
    /// Exec `cmd` in `container` and capture combined stdout/stderr.
    async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String>;
}
