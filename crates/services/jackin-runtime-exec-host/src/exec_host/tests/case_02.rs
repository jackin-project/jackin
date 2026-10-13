// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn start_bound_for_container_creates_host_sock_before_returning() {
    let temp = tempfile::tempdir().unwrap();
    let handle = start_bound_for_container(temp.path(), "fixture", &[]).unwrap();
    let sock = temp
        .path()
        .join("sockets")
        .join("fixture")
        .join("host.sock");
    assert!(sock.exists());
    handle.abort();
}
