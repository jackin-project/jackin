// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Backend-absent purge guards and socket dir removal.

#![expect(
    clippy::print_stderr,
    reason = "runtime cleanup and GC report operator-visible warnings and results"
)]

use jackin_core::JackinPaths;

use jackin_docker::docker_client::DockerApi;

use crate::runtime::backend::{ContainerBackend as _, InstanceBackend};

pub(crate) async fn ensure_backend_absent_for_purge(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    match crate::runtime::backend::backend_for_state(paths, container_name) {
        InstanceBackend::Docker => {
            crate::runtime::backend::DockerBackend::new(docker)
                .ensure_absent_for_purge(paths, container_name)
                .await
        }
        InstanceBackend::AppleContainer => {
            crate::runtime::backend::AppleContainerBackend::production()
                .ensure_absent_for_purge(paths, container_name)
                .await
        }
    }
}

/// Remove the host-side bind-mount directory used to expose the daemon
/// socket and Capsule launch config into the container. Best-effort:
/// any failure is logged to stderr but does not abort the surrounding
/// teardown — the docker-side resources are already gone, and a
/// half-removed `~/.jackin/sockets/<container>/` is no worse than the
/// pre-fix steady state.
pub(crate) async fn remove_socket_dir(paths: &JackinPaths, container_name: &str) {
    let paths = paths.clone();
    let dir = paths.jackin_home.join("sockets").join(container_name);
    let displayed = dir.clone();
    let result = jackin_telemetry::spawn::joined_blocking(move || {
        crate::runtime::coordination::ensure_prunable(&paths, &dir).and_then(|()| {
            crate::isolation::safe_remove::safe_remove_dir_contained(&paths.jackin_home, &dir)
        })
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
