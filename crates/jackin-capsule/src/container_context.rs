// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container context: identity metadata (container name, instance ID,
//! invocation identity) about the running container, available to the daemon.
//!
//! Not responsible for: role/workspace config (see `config`) or attach-session
//! state (see `attach_context`).

use jackin_core::instance_id_from_container_base as instance_id_from_container_name;

pub const JACKIN_CONTAINER_NAME_ENV: &str = "JACKIN_CONTAINER_NAME";
pub const JACKIN_INSTANCE_ID_ENV: &str = "JACKIN_INSTANCE_ID";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusIdentity {
    pub container_name: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDiagnostics {
    pub host_version: String,
    pub invocation_id: String,
}

impl Default for ContainerDiagnostics {
    fn default() -> Self {
        Self {
            host_version: "unknown".to_owned(),
            invocation_id: String::new(),
        }
    }
}

pub fn resolve_status_identity() -> StatusIdentity {
    let container_name = resolve_container_name();
    let instance_id = resolve_instance_id(&container_name);
    StatusIdentity {
        container_name,
        instance_id,
    }
}

pub fn resolve_container_diagnostics() -> ContainerDiagnostics {
    let host_version =
        std::env::var("JACKIN_HOST_VERSION").unwrap_or_else(|_| "unknown".to_owned());
    let invocation_id = std::env::var("JACKIN_INVOCATION_ID").unwrap_or_default();
    ContainerDiagnostics {
        host_version,
        invocation_id,
    }
}

fn resolve_container_name() -> String {
    if let Some(value) = std::env::var(JACKIN_CONTAINER_NAME_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return value;
    }
    if let Some(value) = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "statusbar: container name resolved from HOSTNAME"
        );
        return value;
    }
    const ETC_HOSTNAME_MAX_BYTES: u64 = 256;
    if let Some(value) = crate::util::read_text_bounded(
        "/etc/hostname",
        std::path::Path::new("/etc/hostname"),
        ETC_HOSTNAME_MAX_BYTES,
    )
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
    {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "statusbar: container name resolved from /etc/hostname"
        );
        return value;
    }
    jackin_diagnostics::telemetry_info!(
        "capsule",
        "statusbar: container name unresolved - {JACKIN_CONTAINER_NAME_ENV}, HOSTNAME, and /etc/hostname all empty or unreadable; chrome chip will be blank"
    );
    String::new()
}

fn resolve_instance_id(container_name: &str) -> String {
    std::env::var(JACKIN_INSTANCE_ID_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            instance_id_from_container_name(container_name)
                .map_or_else(|| container_name.to_owned(), str::to_owned)
        })
}
