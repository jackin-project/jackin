// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const SOCKET_WIRE_CHILD: &str = "JACKIN_SOCKET_WIRE_CHILD";

pub(super) const SOCKET_WIRE_TEST: &str =
    "socket::tests::case_01::conformance_wire_real_listener_has_bounded_private_open_and_close";

pub(super) fn dispatch_socket_wire_child() -> Result<bool> {
    if std::env::var_os(SOCKET_WIRE_CHILD).is_some() {
        return Ok(false);
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", SOCKET_WIRE_TEST, "--nocapture"])
        .env(SOCKET_WIRE_CHILD, "1")
        .status()?;
    anyhow::ensure!(status.success(), "isolated socket wire test failed");
    Ok(true)
}
