// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker IPC client.

use std::collections::BTreeMap;
use std::fs;

use std::io::Write;

use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
use std::os::unix::net::UnixStream;
use std::path::{Component, Path, PathBuf};

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability,
    UsageBrokerOperation, UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError,
    UsageCredentialScope, UsageGenerationView, UsageProjectionV1, UsageRelayCapabilityResolutionV1,
};
use jackin_protocol::usage_monitor::{
    MonitorIssue, MonitorIssueCode, MonitorOperation, MonitorReply,
};

use crate::{
    BROKER_DIR, BROKER_RUN_DIR, BROKER_SOCKET_ALIAS_DIR_PREFIX, ForwardedUsageSources,
    monitor::MONITOR_WATCH_TIMEOUT_CAP_MS, protocol_error, read_frame, unavailable,
};
use nix::unistd::geteuid;

/// Small synchronous client. Each operation uses one bounded frame/connection.
///
/// A client also carries one monitoring screen's subscription set (see
/// `view`). Cloning forks that set: the clone starts with the same observed
/// generations but later (un)subscribes diverge.
#[derive(Debug)]
pub struct UsageBrokerClient {
    pub(crate) socket_path: PathBuf,
    pub(crate) build_id: String,
    pub(crate) host_data_dir: Option<PathBuf>,
    pub(crate) subscriptions: Arc<Mutex<BTreeMap<UsageAccountCapability, u64>>>,
}

impl Clone for UsageBrokerClient {
    fn clone(&self) -> Self {
        let subscriptions = self
            .subscriptions
            .lock()
            .map(|subscriptions| subscriptions.clone())
            .unwrap_or_default();
        Self {
            socket_path: self.socket_path.clone(),
            build_id: self.build_id.clone(),
            host_data_dir: self.host_data_dir.clone(),
            subscriptions: Arc::new(Mutex::new(subscriptions)),
        }
    }
}

impl UsageBrokerClient {
    /// Attach to an already-running broker socket.
    #[must_use]
    pub fn at(socket_path: PathBuf, build_id: String) -> Self {
        Self {
            socket_path,
            build_id,
            host_data_dir: None,
            subscriptions: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub(crate) fn at_host(socket_path: PathBuf, data_dir: PathBuf, build_id: String) -> Self {
        let mut client = Self::at(socket_path, build_id);
        client.host_data_dir = Some(data_dir);
        client
    }

    /// Attach to the current Capsule's per-container relay.
    #[must_use]
    pub fn scoped_relay() -> Self {
        Self::at(
            PathBuf::from(jackin_core::container_paths::USAGE_SOCK),
            env!("CARGO_PKG_VERSION").to_owned(),
        )
    }

    /// Read one account generation without provider work.
    pub fn current(
        &self,
        capability: UsageAccountCapability,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::Current { capability })
    }

    /// Read one exact account capability through a scoped relay.
    pub fn current_for_capability(
        &self,
        capability: UsageAccountCapability,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::CurrentForCapability { capability })
    }

    /// Request or join one account generation.
    pub fn refresh(
        &self,
        capability: UsageAccountCapability,
        observed_generation: u64,
        force: bool,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::Refresh {
            capability,
            observed_generation,
            force,
        })
    }

    /// Request or join one exact account capability through a scoped relay.
    pub fn refresh_for_capability(
        &self,
        capability: UsageAccountCapability,
        observed_generation: u64,
        force: bool,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::RefreshForCapability {
            capability,
            observed_generation,
            force,
        })
    }

    /// Wait for one named generation. This does not release broker ownership on timeout.
    pub fn join(
        &self,
        capability: UsageAccountCapability,
        generation: u64,
        timeout: Duration,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.execute(UsageBrokerOperation::Join {
            capability,
            generation,
            timeout_ms,
        })
    }

    /// Wait for one exact capability generation through a scoped relay.
    pub fn join_for_capability(
        &self,
        capability: UsageAccountCapability,
        generation: u64,
        timeout: Duration,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.execute(UsageBrokerOperation::JoinForCapability {
            capability,
            generation,
            timeout_ms,
        })
    }

    /// Execute one already-authorized broker operation.
    pub fn execute(
        &self,
        operation: UsageBrokerOperation,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute_with_scope(operation, None)
    }

    /// Execute one operation with the immutable launch proof supplied by the
    /// host relay. The caller cannot replace this with Capsule input because
    /// the relay owns the client invocation.
    pub fn execute_scoped(
        &self,
        operation: UsageBrokerOperation,
        scope: UsageCredentialScope,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute_with_scope(operation, Some(scope))
    }

    /// Submit one host-only monitor operation over the passive broker socket.
    ///
    /// This method never starts the broker, resolves credentials, or performs
    /// provider work. Callers that need a service must activate it explicitly
    /// through the host lifecycle API first.
    pub fn monitor(&self, request: MonitorOperation) -> Result<MonitorReply, MonitorIssue> {
        let watch_timeout = match &request {
            MonitorOperation::Watch { timeout_ms, .. } => {
                (*timeout_ms).min(MONITOR_WATCH_TIMEOUT_CAP_MS)
            }
            _ => 0,
        };
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: self.build_id.clone(),
            operation: UsageBrokerOperation::Monitor { request },
            launch_credential_scope: None,
        };
        let mut bytes = serde_json::to_vec(&request).map_err(|_| monitor_unavailable())?;
        if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
            return Err(monitor_unavailable());
        }
        bytes.push(b'\n');
        self.validate_socket_path()
            .map_err(|_| monitor_unavailable())?;
        let mut stream =
            UnixStream::connect(&self.socket_path).map_err(|_| monitor_unavailable())?;
        let read_timeout =
            Duration::from_millis(watch_timeout).saturating_add(Duration::from_secs(5));
        stream
            .set_read_timeout(Some(read_timeout.max(Duration::from_secs(5))))
            .map_err(|_| monitor_unavailable())?;
        stream
            .write_all(&bytes)
            .map_err(|_| monitor_unavailable())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|_| monitor_unavailable())?;
        match read_frame::<UsageBrokerResponse>(&mut stream).map_err(|_| monitor_unavailable())? {
            UsageBrokerResponse::Monitor { reply } => Ok(reply),
            UsageBrokerResponse::MonitorError { issue } => Err(issue),
            UsageBrokerResponse::Error { .. }
            | UsageBrokerResponse::State { .. }
            | UsageBrokerResponse::Projection { .. }
            | UsageBrokerResponse::RelayCapabilities { .. } => Err(monitor_unavailable()),
        }
    }

    /// Resolve one Capsule launch against discovery owned by the host broker.
    ///
    /// The source facts are secret-free. This call may inspect config and
    /// credentials inside an already-running provider broker, so it is a
    /// launch operation and never starts the broker implicitly.
    pub fn resolve_relay_capabilities(
        &self,
        scope_label: &str,
        forwarded_sources: &ForwardedUsageSources,
    ) -> Result<UsageRelayCapabilityResolutionV1, UsageCoordinationError> {
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: self.build_id.clone(),
            operation: UsageBrokerOperation::ResolveRelayCapabilities {
                scope_label: scope_label.to_owned(),
                forwarded_sources: forwarded_sources.into(),
            },
            launch_credential_scope: None,
        };
        let mut bytes = serde_json::to_vec(&request).map_err(|_| unavailable())?;
        if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
            return Err(protocol_error());
        }
        bytes.push(b'\n');
        self.validate_socket_path()?;
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|_| unavailable())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| unavailable())?;
        stream.write_all(&bytes).map_err(|_| unavailable())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|_| unavailable())?;
        match read_frame::<UsageBrokerResponse>(&mut stream)? {
            UsageBrokerResponse::RelayCapabilities { resolution } => Ok(*resolution),
            UsageBrokerResponse::Error { error } => Err(error),
            UsageBrokerResponse::State { .. }
            | UsageBrokerResponse::Projection { .. }
            | UsageBrokerResponse::Monitor { .. }
            | UsageBrokerResponse::MonitorError { .. } => Err(protocol_error()),
        }
    }

    pub(crate) fn execute_with_scope(
        &self,
        operation: UsageBrokerOperation,
        launch_credential_scope: Option<UsageCredentialScope>,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: self.build_id.clone(),
            operation,
            launch_credential_scope,
        };
        let mut bytes = serde_json::to_vec(&request).map_err(|_| unavailable())?;
        if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
            return Err(protocol_error());
        }
        bytes.push(b'\n');
        self.validate_socket_path()?;
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|_| unavailable())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| unavailable())?;
        stream.write_all(&bytes).map_err(|_| unavailable())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|_| unavailable())?;
        let response = read_frame::<UsageBrokerResponse>(&mut stream)?;
        match response {
            UsageBrokerResponse::State { state } => Ok(*state),
            UsageBrokerResponse::Projection { .. }
            | UsageBrokerResponse::Monitor { .. }
            | UsageBrokerResponse::RelayCapabilities { .. }
            | UsageBrokerResponse::MonitorError { .. } => Err(protocol_error()),
            UsageBrokerResponse::Error { error } => Err(error),
        }
    }

    /// Read the latest canonical publication without dispatching provider work.
    pub fn current_projection(&self) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.execute_projection(UsageBrokerOperation::CurrentProjection)
    }

    /// Request or join one broker-owned canonical projection refresh.
    pub fn request_refresh(
        &self,
        observed_projection_id: Option<String>,
        force: bool,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.execute_projection(UsageBrokerOperation::RequestRefresh {
            force,
            observed_projection_id,
        })
    }

    /// Join one immutable canonical publication without cancelling shared work.
    pub fn join_publication(
        &self,
        projection_id: String,
        timeout: Duration,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.execute_projection(UsageBrokerOperation::JoinPublication {
            projection_id,
            timeout_ms,
        })
    }

    pub(crate) fn execute_projection(
        &self,
        operation: UsageBrokerOperation,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.execute_projection_with_timeout(operation, Duration::from_secs(30))
    }

    pub(crate) fn probe_current_projection(&self) -> bool {
        self.execute_projection_with_timeout(
            UsageBrokerOperation::CurrentProjection,
            Duration::from_secs(1),
        )
        .is_ok()
    }

    fn execute_projection_with_timeout(
        &self,
        operation: UsageBrokerOperation,
        read_timeout: Duration,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: self.build_id.clone(),
            operation,
            launch_credential_scope: None,
        };
        let mut bytes = serde_json::to_vec(&request).map_err(|_| unavailable())?;
        if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
            return Err(protocol_error());
        }
        bytes.push(b'\n');
        self.validate_socket_path()?;
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|_| unavailable())?;
        stream
            .set_read_timeout(Some(read_timeout))
            .map_err(|_| unavailable())?;
        stream.write_all(&bytes).map_err(|_| unavailable())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|_| unavailable())?;
        match read_frame::<UsageBrokerResponse>(&mut stream)? {
            UsageBrokerResponse::Projection { projection } => Ok(*projection),
            UsageBrokerResponse::Error { error } => Err(error),
            UsageBrokerResponse::State { .. }
            | UsageBrokerResponse::Monitor { .. }
            | UsageBrokerResponse::RelayCapabilities { .. }
            | UsageBrokerResponse::MonitorError { .. } => Err(protocol_error()),
        }
    }

    fn validate_socket_path(&self) -> Result<(), UsageCoordinationError> {
        if self
            .socket_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(unavailable());
        }
        let socket = fs::symlink_metadata(&self.socket_path).map_err(|_| unavailable())?;
        if socket.file_type().is_symlink()
            || !socket.file_type().is_socket()
            || socket.uid() != geteuid().as_raw()
            || socket.mode() & 0o777 != 0o600
        {
            return Err(unavailable());
        }
        if let Some(data_dir) = &self.host_data_dir {
            validate_broker_data_tree(data_dir)?;
        }
        let parent = self.socket_path.parent().ok_or_else(unavailable)?;
        let parent_name = parent.file_name().and_then(|name| name.to_str());
        if parent_name == Some(BROKER_RUN_DIR)
            && parent
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some(BROKER_DIR)
        {
            validate_private_directory(parent)?;
            let broker_dir = parent.parent().ok_or_else(unavailable)?;
            validate_private_directory(broker_dir)?;
            let data_dir = broker_dir.parent().ok_or_else(unavailable)?;
            validate_base_directory(data_dir)?;
            validate_broker_data_ancestors(data_dir)?;
            return Ok(());
        }
        let alias_dir = format!("{BROKER_SOCKET_ALIAS_DIR_PREFIX}{}", geteuid().as_raw());
        if parent_name == Some(alias_dir.as_str()) {
            validate_private_directory(parent)?;
            validate_trusted_ancestors(parent.parent().ok_or_else(unavailable)?)?;
            return Ok(());
        }
        if self.socket_path.as_path() == Path::new(jackin_core::container_paths::USAGE_SOCK) {
            validate_runtime_relay_directory(parent)?;
            validate_trusted_ancestors(parent.parent().ok_or_else(unavailable)?)?;
            return Ok(());
        }
        validate_private_directory(parent)?;
        validate_trusted_ancestors(parent.parent().ok_or_else(unavailable)?)
    }
}

pub(crate) fn validate_broker_data_ancestors(
    data_dir: &Path,
) -> Result<(), UsageCoordinationError> {
    if data_dir
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(unavailable());
    }
    let absolute = if data_dir.is_absolute() {
        data_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| unavailable())?
            .join(data_dir)
    };
    validate_trusted_ancestors(absolute.parent().ok_or_else(unavailable)?)
}

pub(crate) fn validate_broker_data_tree(data_dir: &Path) -> Result<(), UsageCoordinationError> {
    validate_broker_data_ancestors(data_dir)?;
    validate_base_directory(data_dir)?;
    let broker_dir = data_dir.join(BROKER_DIR);
    validate_private_directory(&broker_dir)?;
    validate_private_directory(&broker_dir.join(BROKER_RUN_DIR))
}

fn validate_runtime_relay_directory(path: &Path) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() || metadata.mode() & 0o777 != 0o700 {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_private_directory(path: &Path) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_base_directory(path: &Path) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_trusted_ancestors(path: &Path) -> Result<(), UsageCoordinationError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| unavailable())?
            .join(path)
    };
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::RootDir => prefix.push(component.as_os_str()),
            Component::CurDir => continue,
            Component::ParentDir => return Err(unavailable()),
            Component::Normal(_) | Component::Prefix(_) => prefix.push(component.as_os_str()),
        }
        let link_metadata = fs::symlink_metadata(&prefix).map_err(|_| unavailable())?;
        let (owner, mode, is_dir) = if link_metadata.file_type().is_symlink() {
            if link_metadata.uid() != 0 && link_metadata.uid() != geteuid().as_raw() {
                return Err(unavailable());
            }
            let target = fs::metadata(&prefix).map_err(|_| unavailable())?;
            (target.uid(), target.mode(), target.is_dir())
        } else {
            (
                link_metadata.uid(),
                link_metadata.mode(),
                link_metadata.is_dir(),
            )
        };
        let root_sticky = owner == 0 && mode & 0o1000 != 0;
        if !is_dir
            || (owner != 0 && owner != geteuid().as_raw())
            || (mode & 0o022 != 0 && !root_sticky)
        {
            return Err(unavailable());
        }
    }
    Ok(())
}

fn monitor_unavailable() -> MonitorIssue {
    MonitorIssue {
        code: MonitorIssueCode::BrokerUnavailable,
        message: "host usage broker is unavailable or returned an incompatible response".to_owned(),
        retry_at_epoch: None,
    }
}
