// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::process::{Command, Stdio};

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};
use jackin_usage_discovery::UsageDiscoveryScope;

use crate::{UsageBrokerClient, UsageBrokerConfig, connect_probe, unavailable, wait_for_leader};

pub fn ensure_usage_broker_process(
    config: UsageBrokerConfig,
    scope: &UsageDiscoveryScope,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    let client = config.client();
    client.initialization_result()?;
    if connect_probe(&client) {
        return Ok(client);
    }
    let executable = config
        .service_executable
        .as_ref()
        .ok_or_else(|| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message:
                "usage broker executable cannot be located; from a source checkout run `mise exec -- mbx +1.99.1 build --bins -p jackin`, or reinstall the complete jackin package"
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
    add_socket_alias_root(&mut command, config.socket_alias_root.as_deref());
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
            "cannot start usage broker executable {}: {error}; from a source checkout run `mise exec -- mbx +1.99.1 build --bins -p jackin`, or reinstall the complete jackin package",
            executable.display(),
        ),
    })?;
    wait_for_leader(&client)?;
    Ok(client)
}

pub fn ensure_usage_monitor_process(
    config: UsageBrokerConfig,
    scope: &UsageDiscoveryScope,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    if !matches!(scope, UsageDiscoveryScope::HostDesktop { .. }) {
        return Err(unavailable());
    }
    let client = config.client();
    client.initialization_result()?;
    if connect_probe(&client) {
        return Ok(client);
    }
    let executable = config
        .service_executable
        .as_ref()
        .ok_or_else(|| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message:
                "usage broker executable cannot be located; from a source checkout run `mise exec -- mbx +1.99.1 build --bins -p jackin`, or reinstall the complete jackin package"
                    .to_owned(),
        })?;
    let mut command = Command::new(executable);
    command
        .arg("--data-dir")
        .arg(&config.data_dir)
        .arg("--build-id")
        .arg(&config.build_id)
        .arg("--local-only")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    add_socket_alias_root(&mut command, config.socket_alias_root.as_deref());
    command
        .spawn()
        .map_err(|_| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: format!(
                "cannot start local-only usage broker executable {}; from a source checkout run `mise exec -- mbx +1.99.1 build --bins -p jackin`, or reinstall the complete jackin package",
                executable.display(),
            ),
        })?;
    wait_for_leader(&client)?;
    Ok(client)
}

fn add_socket_alias_root(command: &mut Command, alias_root: Option<&Path>) {
    if let Some(alias_root) = alias_root {
        command.arg("--socket-alias-root").arg(alias_root);
    }
}

#[cfg(test)]
mod tests;
