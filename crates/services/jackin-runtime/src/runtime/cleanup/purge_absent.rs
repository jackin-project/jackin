// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Backend-absent purge guards.
//!
//! Socket-dir removal lives in
//! `jackin_runtime_cleanup_socket_dir` (S7 split 105),
//! re-exported below.

use jackin_core::JackinPaths;

use jackin_docker::docker_client::DockerApi;

use crate::runtime::backend::{ContainerBackend as _, InstanceBackend};

// Moved to jackin_runtime_cleanup_socket_dir::socket_dir (S7 split 105);
// the item re-export keeps every `cleanup::remove_socket_dir` path stable.
pub(crate) use jackin_runtime_cleanup_socket_dir::socket_dir::remove_socket_dir;

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
