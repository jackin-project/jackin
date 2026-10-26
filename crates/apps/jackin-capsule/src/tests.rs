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

#[test]
fn usage_relay_proxy_v2_is_client_not_daemon_entrypoint() {
    let args = ["jackin-capsule", "usage-relay-proxy-v2"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(!is_daemon_entrypoint_args(&args));
    usage_relay_proxy::validate_v2_args(&[])
        .expect("empty v2 arguments select the default command");
    assert!(usage_relay_proxy::validate_v2_args(&["--help".to_owned()]).is_err());
}
