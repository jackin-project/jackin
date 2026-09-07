// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Runtime outcome handoff and failure cleanup ownership.

use super::{RuntimeLaunched, handle_launch_failure};
use jackin_docker::docker_client::DockerApi;

pub(super) enum RuntimeDispatch {
    AppleContainer(String),
    Detached(String),
    Docker(Box<RuntimeLaunched>),
}

impl RuntimeDispatch {
    fn from_docker_outcome(
        outcome: crate::runtime::launch::launch_runtime::LaunchOutcome,
        container_name: &str,
        mut launched: RuntimeLaunched,
    ) -> Self {
        use crate::runtime::launch::launch_runtime::LaunchOutcome;

        match outcome {
            LaunchOutcome::Detached => {
                // Ownership passes to the running instance. Only a later attach
                // or eject may decide to finalize its resources.
                launched.cleanup.disarm();
                Self::Detached(container_name.to_owned())
            }
            LaunchOutcome::ForegroundSessionEnded => {
                launched.cleanup.keep_socket_dir();
                Self::Docker(Box::new(launched))
            }
        }
    }
}

pub(super) async fn complete_docker_launch(
    result: anyhow::Result<crate::runtime::launch::launch_runtime::LaunchOutcome>,
    mut launched: RuntimeLaunched,
    paths: &jackin_core::JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<RuntimeDispatch> {
    if result.is_err() {
        handle_launch_failure(
            paths,
            &launched.container_state,
            &mut launched.instance_manifest,
            container_name,
            &launched.cleanup,
            docker,
        )
        .await;
    }
    Ok(RuntimeDispatch::from_docker_outcome(
        result?,
        container_name,
        launched,
    ))
}
