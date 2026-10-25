// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::CapsuleConfig;

use crate::usage_relay::{docker_runtime_mount, resolved_launch_usage_inventory};

#[test]
fn docker_mount_uses_container_run_dir() -> anyhow::Result<()> {
    assert_eq!(
        docker_runtime_mount(std::path::Path::new("/sockets/relay"))?,
        "/sockets/relay:/jackin/run"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn docker_mount_rejects_non_utf8_socket_dir() {
    use std::os::unix::ffi::OsStrExt as _;
    let raw = std::ffi::OsStr::from_bytes(b"/sockets/\xffrelay");
    let rejected = docker_runtime_mount(std::path::Path::new(raw)).is_err();
    assert!(rejected);
}

#[test]
fn inventory_sorts_and_deduplicates_instances() {
    let config = CapsuleConfig {
        instances: vec!["beta".to_owned(), "alpha".to_owned(), "beta".to_owned()],
        ..Default::default()
    };
    assert_eq!(
        resolved_launch_usage_inventory(&config).instances,
        vec!["alpha".to_owned(), "beta".to_owned()]
    );
}
