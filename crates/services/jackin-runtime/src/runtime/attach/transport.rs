// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host attach transport selection and `attach-proxy` exec args.

use jackin_core::ContainerHandle;
use jackin_core::container_paths;

use jackin_core::JackinPaths;
use std::path::PathBuf;

use super::capsule_socket_negotiates;

/// Shell command for querying the in-container daemon's session
/// inventory.
///
/// Gated on the daemon's socket file (`/jackin/run/jackin.sock`) so
/// the early-bring-up window — between container start and
/// `setup-once` finishing + the daemon binding its socket — does not
/// emit a wave of operator-visible stderr from a binary that exists
/// but cannot serve yet. `test -S` exits silently with status 1 if
/// the socket is absent, which `exec_capture` surfaces as `Err` and
/// callers route through `AgentSessionInventory::Unavailable`. Once
/// the socket is bound, every real failure mode of the status call
/// (daemon crashed mid-request, oversize reply, garbled JSON)
/// propagates loudly because `||` short-circuits at the first failure
/// only — there is no `|| true` suppression of the second command's
/// errors.
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

/// Conservative `sockaddr_un.sun_path` capacity across the platforms jackin'
/// targets (macOS/BSD = 104, Linux = 108). A socket path at or above this cannot
/// be `connect`ed directly — the kernel rejects it — so the direct transport is
/// impossible regardless of whether the socket exists.
pub(crate) const MAX_UNIX_SOCKET_PATH_LEN: usize = 104;

pub fn select_host_attach_transport(
    paths: &JackinPaths,
    container_name: &str,
) -> HostAttachTransportPlan {
    let socket_path = crate::runtime::snapshot::socket_path(paths, container_name);

    // A path at/over the `sun_path` limit can never bind/connect directly; the OS
    // returns a generic error that reads like "connection refused", silently
    // degrading to the attach-proxy and conflating "too long" with "not ready"
    // (Bug 10). Detect it explicitly and surface it at a visible tier with a
    // precise reason, instead of leaving it to a swallowed connect error.
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
