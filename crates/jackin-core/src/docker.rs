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

/// Immutable Docker identity captured from one daemon inspection.
///
/// Container names are mutable namespace entries. Lifecycle operations must use
/// this handle's daemon-assigned ID after the name lookup so a replacement with
/// the same name cannot receive an operation intended for the original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHandle {
    /// Container name captured with the ID for diagnostics and lookup context.
    pub name: String,
    /// Immutable daemon-assigned container ID.
    pub id: String,
}

impl ContainerHandle {
    /// Build a handle from a daemon-assigned ID and its lookup name.
    #[must_use]
    pub fn new(name: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: id.into(),
        }
    }
}

/// Result of resolving a container name before a lifecycle operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInspection {
    /// Immutable handle when the named container exists.
    pub handle: Option<ContainerHandle>,
    /// State observed during the same daemon inspection.
    pub state: ContainerState,
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
    /// Resolve a container name and capture its immutable daemon ID.
    #[must_use]
    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection;
    /// Inspect a container by immutable daemon ID.
    #[must_use]
    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState;
    /// Inspect a container by name, then inspect it by its captured ID.
    #[must_use]
    async fn inspect_container_state(&self, name: &str) -> ContainerState {
        let inspection = self.inspect_container_by_name(name).await;
        match inspection.handle {
            Some(container) => self.inspect_container_by_id(&container).await,
            None => inspection.state,
        }
    }
    /// Force-remove a container by immutable daemon ID.
    async fn remove_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()>;
    /// Resolve a name, then force-remove only the captured container ID.
    async fn remove_container(&self, name: &str) -> anyhow::Result<()> {
        let inspection = self.inspect_container_by_name(name).await;
        let Some(container) = inspection.handle else {
            return match inspection.state {
                ContainerState::NotFound => Ok(()),
                state => anyhow::bail!(
                    "cannot remove container {name}: {}",
                    state.inspect_label()
                ),
            };
        };
        self.remove_container_by_id(&container).await
    }
    /// List containers matching label filters; `all` includes stopped ones.
    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>>;
    /// Create a container with `name` from `spec` and return its immutable ID.
    async fn create_container(
        &self,
        name: &str,
        spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle>;
    /// Start a previously created container by immutable daemon ID.
    async fn start_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()>;
    /// Resolve a name, then start only the captured container ID.
    async fn start_container(&self, name: &str) -> anyhow::Result<()> {
        let inspection = self.inspect_container_by_name(name).await;
        let Some(container) = inspection.handle else {
            anyhow::bail!("cannot start missing container {name}");
        };
        self.start_container_by_id(&container).await
    }
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
    /// Exec `cmd` in a container selected by immutable daemon ID.
    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String>;
    /// Resolve a name, then exec only in the captured container ID.
    async fn exec_capture(
        &self,
        name: &str,
        cmd: &[&str],
    ) -> anyhow::Result<String> {
        let inspection = self.inspect_container_by_name(name).await;
        let Some(container) = inspection.handle else {
            anyhow::bail!("cannot exec in missing container {name}");
        };
        self.exec_capture_by_id(&container, cmd).await
    }
}
