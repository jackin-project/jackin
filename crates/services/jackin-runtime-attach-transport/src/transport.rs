// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::ContainerHandle;
use jackin_core::container_paths;

use jackin_core::JackinPaths;
use std::path::PathBuf;

use jackin_runtime_attach_capsule_ready::capsule_ready::capsule_socket_negotiates;

pub const JACKIN_CAPSULE_PATH: &str = container_paths::CAPSULE_BIN;
pub const ATTACH_PROXY_SUBCOMMAND: &str = "attach-proxy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostAttachTransportPlan {
    DirectSocket {
        socket_path: PathBuf,
    },
    AttachProxy {
        socket_path: PathBuf,
        direct_error: Option<String>,
    },
}

pub fn attach_proxy_exec_args(container: &ContainerHandle) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "-i".to_owned(),
        container.id().to_owned(),
        JACKIN_CAPSULE_PATH.to_owned(),
        ATTACH_PROXY_SUBCOMMAND.to_owned(),
    ]
}

pub const MAX_UNIX_SOCKET_PATH_LEN: usize = 104;

pub fn select_host_attach_transport(
    paths: &JackinPaths,
    container_name: &str,
) -> HostAttachTransportPlan {
    let socket_path = jackin_runtime_snapshot::snapshot::socket_path(paths, container_name);

    let path_len = socket_path.as_os_str().len();
    if path_len >= MAX_UNIX_SOCKET_PATH_LEN {
        let reason = format!(
            "socket path is {path_len} bytes, at/over the {MAX_UNIX_SOCKET_PATH_LEN}-byte \
             sun_path limit; using attach-proxy (shorten the jackin state dir)"
        );
        let _warning = jackin_telemetry::record_recovered_degradation();
        return HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error: Some(reason),
        };
    }

    if !socket_path.exists() {
        return HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error: None,
        };
    }

    match capsule_socket_negotiates(&socket_path) {
        Ok(()) => HostAttachTransportPlan::DirectSocket { socket_path },
        Err(err) => HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error: Some(err.to_string()),
        },
    }
}
