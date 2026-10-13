// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Synchronous client for the host usage broker.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability,
    UsageBrokerOperation, UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError,
    UsageCredentialScope, UsageGenerationView, UsageProjectionV1,
};

use super::{protocol_error, read_frame, unavailable};

/// Small synchronous client. Each operation uses one bounded frame/connection.
///
/// A client also carries one monitoring screen's subscription set (see
/// `view`). Cloning forks that set: the clone starts with the same observed
/// generations but later (un)subscribes diverge.
#[derive(Debug)]
pub struct UsageBrokerClient {
    pub(super) socket_path: PathBuf,
    pub(super) build_id: String,
    pub(super) host_data_dir: Option<PathBuf>,
    pub(super) subscriptions: Arc<Mutex<BTreeMap<UsageAccountCapability, u64>>>,
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

    pub(super) fn at_host(socket_path: PathBuf, data_dir: PathBuf, build_id: String) -> Self {
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

    fn execute_with_scope(
        &self,
        operation: UsageBrokerOperation,
        launch_credential_scope: Option<UsageCredentialScope>,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.validate_socket_path()?;
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
            | UsageBrokerResponse::MonitorError { .. }
            | UsageBrokerResponse::RelayCapabilities { .. } => Err(protocol_error()),
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

    fn execute_projection(
        &self,
        operation: UsageBrokerOperation,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.execute_projection_with_timeout(operation, Duration::from_secs(30))
    }

    fn execute_projection_with_timeout(
        &self,
        operation: UsageBrokerOperation,
        read_timeout: Duration,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.validate_socket_path()?;
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
            | UsageBrokerResponse::MonitorError { .. }
            | UsageBrokerResponse::RelayCapabilities { .. } => Err(protocol_error()),
        }
    }

    pub(super) fn probe_current_projection(&self) -> bool {
        self.execute_projection_with_timeout(
            UsageBrokerOperation::CurrentProjection,
            Duration::from_secs(1),
        )
        .is_ok()
    }
}
