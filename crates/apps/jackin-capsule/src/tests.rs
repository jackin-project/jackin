// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_relay_proxy_is_client_not_daemon_entrypoint() {
    let args = ["jackin-capsule", "usage-relay-proxy"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(!is_daemon_entrypoint_args(&args));
}
