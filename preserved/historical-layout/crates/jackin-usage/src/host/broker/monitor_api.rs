// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Attach-only monitor IPC.

use std::time::Duration;

use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageBrokerOperation,
    UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError,
    UsageRelayCapabilityResolutionV1, UsageRelayForwardedSourcesV1,
};
use jackin_protocol::usage_monitor::{
    MonitorIssue, MonitorIssueCode, MonitorOperation, MonitorReply,
};
use std::io::Write;
use std::os::unix::net::UnixStream;

use super::{ForwardedUsageSources, UsageBrokerClient, protocol_error, read_frame, unavailable};

const MONITOR_WATCH_TIMEOUT_CAP_MS: u64 = 30_000;

impl From<&ForwardedUsageSources> for UsageRelayForwardedSourcesV1 {
    fn from(sources: &ForwardedUsageSources) -> Self {
        Self {
            selected_account_ids: sources.selected_account_ids.clone(),
            selected_account_surfaces: sources.selected_account_surfaces.clone(),
            profile_surface_ids: sources.profile_surface_ids.clone(),
            env_keys: sources.env_keys.clone(),
            credential_scope: sources.credential_scope.clone(),
        }
    }
}

impl From<UsageRelayForwardedSourcesV1> for ForwardedUsageSources {
    fn from(sources: UsageRelayForwardedSourcesV1) -> Self {
        Self {
            selected_account_ids: sources.selected_account_ids,
            selected_account_surfaces: sources.selected_account_surfaces,
            profile_surface_ids: sources.profile_surface_ids,
            env_keys: sources.env_keys,
            credential_scope: sources.credential_scope,
        }
    }
}

impl UsageBrokerClient {
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

    /// Resolve a Capsule launch against discovery owned by the host broker.
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
                forwarded_sources: UsageRelayForwardedSourcesV1::from(forwarded_sources),
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
}

fn monitor_unavailable() -> MonitorIssue {
    MonitorIssue {
        code: MonitorIssueCode::BrokerUnavailable,
        message: "host usage broker is unavailable or returned an incompatible response".to_owned(),
        retry_at_epoch: None,
    }
}
