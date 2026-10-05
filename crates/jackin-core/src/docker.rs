// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `DockerApi` trait and pure data types for container operations.
//!
//! This module contains only the trait definition and associated data types —
//! no bollard, no tokio, no Docker daemon connection. The concrete
//! `BollardDockerClient` implementation lives in the binary crate
//! (`docker_client/mod.rs`) until it migrates to `jackin-runtime`.

use std::collections::HashMap;

/// Exact opaque identity reported by Docker's `/info` endpoint.
/// Controller paths and connection endpoints cannot substitute for this ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DaemonServerId(String);

impl DaemonServerId {
    /// Validate a daemon-reported ID, preserving its exact bytes.
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !input.trim().is_empty() && input.len() <= 256 && !input.chars().any(char::is_control),
            "Docker daemon server ID must be nonblank, at most 256 bytes, and contain no control characters"
        );
        Ok(Self(input.to_owned()))
    }

    /// Borrow the exact identity reported by the daemon.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DaemonServerId {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<DaemonServerId> for String {
    fn from(value: DaemonServerId) -> Self {
        value.0
    }
}


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

/// Full Docker identity captured from one daemon inspection or create.
///
/// Container names are mutable. Engine path parameters accept either an ID
/// or a name, so callers verify the returned ID before each operation and
/// retain the residual race between verification and a later delete request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHandle {
    name: String,
    id: String,
}

impl ContainerHandle {
    /// Validate and build a handle from a daemon-assigned ID and its lookup
    /// name.
    pub fn new(name: impl Into<String>, id: impl Into<String>) -> anyhow::Result<Self> {
        let name = name.into();
        let id = id.into();
        anyhow::ensure!(!name.is_empty(), "Docker container name is empty");
        anyhow::ensure!(
            id.len() == 64
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "Docker container ID must be 64 lowercase hexadecimal characters"
        );
        Ok(Self { name, id })
    }

    /// The name captured when this handle was resolved.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The immutable daemon-assigned container ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
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

/// One container row from a list/filter query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRow {
    /// Docker container name (without leading `/` when normalized).
    pub name: String,
    /// Immutable daemon-assigned container ID.
    pub id: String,
    /// Container labels as returned by the daemon.
    pub labels: HashMap<String, String>,
}

impl ContainerRow {
    /// Return the immutable handle represented by this daemon list row.
    pub fn handle(&self) -> anyhow::Result<ContainerHandle> {
        ContainerHandle::new(self.name.clone(), self.id.clone())
    }
}

/// Named Docker volume metadata. Docker volumes have no immutable daemon ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeRow {
    /// Docker volume name.
    pub name: String,
    /// Ownership metadata returned by the daemon.
    pub labels: HashMap<String, String>,
    /// Docker volume driver.
    pub driver: String,
}

/// One network row from a list/filter query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRow {
    /// Immutable daemon-assigned Docker network identity.
    pub id: NetworkId,
    /// Docker network name.
    pub name: String,
    /// Network labels as returned by the daemon.
    pub labels: HashMap<String, String>,
}

/// Immutable full Docker network ID; names and abbreviated IDs are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NetworkId(String);

impl NetworkId {
    /// Validate a daemon-assigned network ID.
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            input.len() == 64 && input.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Docker network ID must be 64 lowercase hexadecimal characters"
        );
        Ok(Self(input.to_owned()))
    }

    /// Borrow the immutable ID for daemon operations.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NetworkId {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<NetworkId> for String {
    fn from(value: NetworkId) -> Self {
        value.0
    }
}

impl std::fmt::Display for NetworkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// User/group for the container's initial process.
    pub user: Option<String>,
    /// Command arguments passed to the image entrypoint.
    pub command: Option<Vec<String>>,
    /// Linux capabilities added to the container.
    pub cap_add: Vec<String>,
    /// Linux capabilities dropped from the container.
    pub cap_drop: Vec<String>,
    /// Mount the container root filesystem read-only.
    pub readonly_rootfs: bool,
    /// Docker security options such as `no-new-privileges`.
    pub security_opt: Vec<String>,
    /// Tmpfs mounts as Docker `path:options` strings.
    pub tmpfs: Vec<String>,
    /// Extra host mappings as Docker `host:ip` strings.
    pub extra_hosts: Vec<String>,
    /// Hard memory limit in bytes.
    pub memory_bytes: Option<u64>,
    /// Soft memory limit in bytes.
    pub memory_reservation_bytes: Option<u64>,
    /// CPU limit in Docker's nanocpu representation.
    pub nano_cpus: Option<i64>,
    /// Process-count limit.
    pub pids_limit: Option<i64>,
    /// Open-file soft and hard limit.
    pub nofile: Option<u64>,
}

/// Async Docker daemon API seam. Dependency-injected so tests can stub Docker
/// without a running daemon.
pub trait DockerApi {
    /// Return the transport captured by the same constructor as this client.
    fn controller_endpoint(&self) -> &ControllerEndpoint;
    /// Ping the daemon (`/_ping`).
    async fn ping(&self) -> anyhow::Result<()>;
    /// Read the current actual server ID from the daemon, without endpoint fallback.
    async fn daemon_server_id(&self) -> anyhow::Result<DaemonServerId>;
    /// Resolve a container name and capture its immutable daemon ID.
    #[must_use]
    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection;
    /// Inspect a container by immutable daemon ID.
    #[must_use]
    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState;
    /// Attest the running init PID for this exact immutable daemon ID.
    /// Backends lacking a host-visible process proof cannot enable credential relays.
    async fn container_init_pid_by_id(&self, container: &ContainerHandle) -> anyhow::Result<u32>;
    /// Force-remove after exact ID preflight. Docker's endpoint is not an
    /// atomic ID-only delete; callers must retain custody on ambiguity.
    async fn remove_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()>;
    /// List containers matching label filters; `all` includes stopped ones.
    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>>;
    /// Create a container with `name` from `spec` (does not start it) and
    /// return its immutable daemon ID.
    async fn create_container(
        &self,
        name: &str,
        spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle>;
    /// Start a previously created container by immutable daemon ID.
    async fn start_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()>;
    /// Create a named volume and reject returned ownership labels that differ.
    async fn create_volume(
        &self,
        name: &str,
        labels: HashMap<String, String>,
    ) -> anyhow::Result<VolumeRow>;
    /// Inspect named volume metadata; `None` when missing.
    async fn inspect_volume_by_name(&self, name: &str) -> anyhow::Result<Option<VolumeRow>>;
    /// Remove a named volume.
    async fn remove_volume(&self, name: &str) -> anyhow::Result<()>;
    /// Create a network and capture its immutable ID; conflicts must fail.
    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<NetworkId>;
    /// Address network removal using its captured full daemon ID. Docker's
    /// endpoint is not an atomic ID-only delete; backends must reject a
    /// mismatched inspection result.
    async fn remove_network_by_id(&self, id: &NetworkId) -> anyhow::Result<()>;
    /// List networks matching label filters.
    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>>;
    /// Inspect a network by name; `None` when missing.
    async fn inspect_network_by_name(&self, name: &str) -> anyhow::Result<Option<NetworkRow>>;
    /// Inspect the exact immutable network ID; `None` when missing.
    async fn inspect_network_by_id(&self, id: &NetworkId) -> anyhow::Result<Option<NetworkRow>>;
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
    /// Exec `cmd` in a container selected by immutable daemon ID and capture
    /// combined stdout/stderr.
    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String>;
}

#[cfg(test)]
mod daemon_server_id_tests {
    use super::DaemonServerId;

    #[test]
    fn preserves_exact_opaque_identity() {
        let raw = " exact/opaque daemon:identity ";
        assert!(matches!(
            DaemonServerId::parse(raw),
            Ok(id)
                if id.as_str() == raw
                    && matches!(
                        serde_json::to_string(&id),
                        Ok(json)
                            if matches!(serde_json::from_str::<DaemonServerId>(&json), Ok(round_trip) if round_trip == id)
                    )
        ));
        assert!(DaemonServerId::parse(&"a".repeat(256)).ok().is_some());
    }

    #[test]
    fn rejects_missing_blank_control_and_oversized_identity() {
        for invalid in [
            "",
            " ",
            "\t",
            "daemon\nidentity",
            "daemon\0identity",
            "daemon\u{7f}identity",
            &"a".repeat(257),
        ] {
            assert!(DaemonServerId::parse(invalid).err().is_some());
            assert!(matches!(
                serde_json::to_string(invalid),
                Ok(json) if serde_json::from_str::<DaemonServerId>(&json).err().is_some()
            ));
        }
        assert!(
            serde_json::from_str::<DaemonServerId>("null")
                .err()
                .is_some()
        );
    }
}
use std::path::PathBuf;

/// Captured controller transport used by this exact Docker client.
/// Admission must inspect this value instead of resolving environment again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControllerEndpoint {
    /// Local daemon Unix socket. Read-only binds still permit socket connections.
    Unix {
        /// Absolute host socket path supplied to the connector.
        socket: PathBuf,
    },
    /// Network daemon; bind checks alone cannot exclude agent access.
    Tcp {
        /// Captured Docker network URI supplied to the connector.
        authority: String,
        /// Captured client authentication files when HTTPS is selected.
        tls: Option<ControllerTlsFiles>,
    },
    /// Windows daemon pipe.
    #[cfg(windows)]
    NamedPipe {
        /// Captured pipe URI supplied to the connector.
        pipe: String,
    },
}

/// Captured TLS file paths consumed by the controller connector.
/// These describe file authority, not immutable credential bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerTlsFiles {
    /// Absolute client private key path.
    pub key: PathBuf,
    /// Absolute client certificate path.
    pub cert: PathBuf,
    /// Absolute daemon certificate authority path.
    pub ca: PathBuf,
}
