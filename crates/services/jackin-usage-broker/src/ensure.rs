// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host broker process activation.

use std::process::{Command, Stdio};

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};
use jackin_usage_discovery::UsageDiscoveryScope;

use crate::{UsageBrokerClient, UsageBrokerConfig, connect_probe, unavailable, wait_for_leader};

/// Activate the independent provider broker executable and attach a client.
///
/// The broker process receives only host discovery paths. It performs
/// discovery, credential resolution, and provider work under its own policy.
pub fn ensure_usage_broker_process(
    config: UsageBrokerConfig,
    scope: &UsageDiscoveryScope,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    let client = config.client();
    if connect_probe(&client) {
        return Ok(client);
    }
    let executable = config
        .service_executable
        .as_ref()
        .ok_or_else(|| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message:
                "usage broker executable cannot be located; from a source checkout run `mise exec -- mbx +1.97.1 build --bins -p jackin`, or reinstall the complete jackin package"
                    .to_owned(),
        })?;
    let mut command = Command::new(executable);
    command
        .arg("--data-dir")
        .arg(&config.data_dir)
        .arg("--build-id")
        .arg(&config.build_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match scope {
        UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home,
        } => {
            command
                .arg("--config-root")
                .arg(config_root)
                .arg("--operator-home")
                .arg(operator_home);
        }
        UsageDiscoveryScope::Capsule { .. } => return Err(unavailable()),
    }
    command.spawn().map_err(|error| UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: format!(
            "cannot start usage broker executable {}: {error}; from a source checkout run `mise exec -- mbx +1.97.1 build --bins -p jackin`, or reinstall the complete jackin package",
            executable.display(),
        ),
    })?;
    wait_for_leader(&client)?;
    Ok(client)
}

/// Start or attach to the host broker for durable monitor work.
///
/// This entry point is safe for explicit monitor or service startup: it never
/// discovers accounts, resolves credentials, or passes operator paths to the
/// service process. All other client operations remain attach-only.
pub fn ensure_usage_monitor_process(
    config: UsageBrokerConfig,
    scope: &UsageDiscoveryScope,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    if !matches!(scope, UsageDiscoveryScope::HostDesktop { .. }) {
        return Err(unavailable());
    }
    let client = config.client();
    if connect_probe(&client) {
        return Ok(client);
    }
    let executable = config
        .service_executable
        .as_ref()
        .ok_or_else(|| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message:
                "usage broker executable cannot be located; from a source checkout run `mise exec -- mbx +1.97.1 build --bins -p jackin`, or reinstall the complete jackin package"
                    .to_owned(),
        })?;
    Command::new(executable)
        .arg("--data-dir")
        .arg(&config.data_dir)
        .arg("--build-id")
        .arg(&config.build_id)
        .arg("--local-only")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: format!(
                "cannot start local-only usage broker executable {}; from a source checkout run `mise exec -- mbx +1.97.1 build --bins -p jackin`, or reinstall the complete jackin package",
                executable.display(),
            ),
        })?;
    wait_for_leader(&client)?;
    Ok(client)
}
