// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side socket-directory removal.
//!
//! [`remove_socket_dir`] removes the per-container `sockets/<name>/`
//! bind-mount directory once the coordination gate confirms nothing
//! prunable is still running, deleting through the contained
//! safe-remove helper. Purge and eject flows in the
//! `jackin-runtime` hub share this step; instance pruning stays in
//! the hub and image, home, role, and cache pruning live in their
//! own leaves.

#![expect(
    clippy::print_stderr,
    reason = "runtime cleanup and GC report operator-visible warnings and results"
)]

use jackin_core::JackinPaths;
use jackin_runtime_coordination::coordination::ensure_prunable;
use jackin_runtime_isolation::isolation::safe_remove::safe_remove_dir_contained;

/// Remove the host-side bind-mount directory used to expose the daemon
/// socket and Capsule launch config into the container. Best-effort:
/// any failure is logged to stderr but does not abort the surrounding
/// teardown — the docker-side resources are already gone, and a
/// half-removed `~/.jackin/sockets/<container>/` is no worse than the
/// pre-fix steady state.
pub async fn remove_socket_dir(paths: &JackinPaths, container_name: &str) {
    let paths = paths.clone();
    let dir = paths.jackin_home.join("sockets").join(container_name);
    let displayed = dir.clone();
    let result = jackin_telemetry::spawn::joined_blocking(move || {
        ensure_prunable(&paths, &dir)
            .and_then(|()| safe_remove_dir_contained(&paths.jackin_home, &dir))
    })
    .await;
    let error = match result {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(error) => std::io::Error::other(error),
    };
    eprintln!(
        "jackin: warning: failed to remove socket dir {}: {error}",
        displayed.display()
    );
}
