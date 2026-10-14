// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::CommandRunner;
use jackin_core::ContainerHandle;
use jackin_diagnostics;
use jackin_docker::docker_client::DockerApi;

use jackin_docker::docker_client::ContainerState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitPhase {
    PreAttach,
    PostAttach,
}

pub async fn diagnose_premature_exit(
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container_name: &str,
    phase: ExitPhase,
) -> Option<anyhow::Error> {
    let state = docker.inspect_container_by_name(container_name).await.state;
    diagnose_with_state(runner, container_name, &state, phase).await
}

pub async fn diagnose_premature_exit_by_id(
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
    phase: ExitPhase,
) -> Option<anyhow::Error> {
    let state = docker.inspect_container_by_id(container).await;
    diagnose_with_state_by_target(runner, container.name(), container.id(), &state, phase).await
}

pub async fn diagnose_with_state_by_id(
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
    state: &ContainerState,
    phase: ExitPhase,
) -> Option<anyhow::Error> {
    diagnose_with_state_by_target(runner, container.name(), container.id(), state, phase).await
}

pub async fn diagnose_with_state(
    runner: &mut impl CommandRunner,
    container_name: &str,
    state: &ContainerState,
    phase: ExitPhase,
) -> Option<anyhow::Error> {
    diagnose_with_state_by_target(runner, container_name, container_name, state, phase).await
}

async fn diagnose_with_state_by_target(
    runner: &mut impl CommandRunner,
    container_name: &str,
    container_ref: &str,
    state: &ContainerState,
    phase: ExitPhase,
) -> Option<anyhow::Error> {
    match state {
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead
        | ContainerState::NotFound
        | ContainerState::InspectUnavailable(_) => None,
        ContainerState::Stopped {
            exit_code,
            oom_killed,
        } => {
            if phase == ExitPhase::PostAttach && *exit_code == 0 && !oom_killed {
                return None;
            }

            let logs = match runner
                .capture_combined("docker", &["logs", "--tail", "40", container_ref], None)
                .await
            {
                Ok(text) => {
                    let trimmed = text.trim().to_owned();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                }
                Err(e) => Some(format!("(docker logs failed: {e:#})")),
            };
            let reason = if *oom_killed {
                "OOM killed".to_owned()
            } else {
                format!("exit {exit_code}")
            };
            let phase_label = match phase {
                ExitPhase::PreAttach => "exited before attach",
                ExitPhase::PostAttach => "exited during session",
            };
            let body = if let Some(text) = logs.as_deref() {
                format!(
                    "container {container_name} {phase_label} ({reason}); last 40 log lines:\n{text}"
                )
            } else {
                format!(
                    "container {container_name} {phase_label} ({reason}) and produced no log output"
                )
            };

            if let Some(run) = jackin_diagnostics::active_run() {
                run.container_exited((*exit_code).into(), *oom_killed);
            }
            Some(anyhow::anyhow!(body))
        }
    }
}

pub fn attach_failure_error(container_name: &str, err: &anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!("capsule attach failed for {container_name}: {err}")
}

pub fn is_known_socket_close(error: &anyhow::Error, state: &ContainerState) -> bool {
    if !matches!(
        state,
        ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }
    ) {
        return false;
    }

    let detail = format!("{error:#}").to_ascii_lowercase();
    detail.contains("early eof")
        || detail.contains("attach socket closed")
        || detail.contains("attach socket eof")
        || (detail.contains("docker exec") && detail.contains("jackin-capsule"))
}

pub async fn inspect_attach_outcome(
    docker: &impl DockerApi,
    container: &str,
) -> anyhow::Result<jackin_isolation::finalize::AttachOutcome> {
    use jackin_isolation::finalize::AttachOutcome;

    Ok(
        match docker.inspect_container_by_name(container).await.state {
            ContainerState::Running
            | ContainerState::Paused
            | ContainerState::Restarting
            | ContainerState::Created
            | ContainerState::Removing => AttachOutcome::still_running(),
            ContainerState::Dead => {
                let _warning = jackin_telemetry::record_recovered_degradation();
                AttachOutcome::still_running()
            }
            ContainerState::Stopped {
                oom_killed: true, ..
            } => AttachOutcome::oom_killed(),
            ContainerState::Stopped { exit_code, .. } => AttachOutcome::stopped(exit_code),
            ContainerState::NotFound | ContainerState::InspectUnavailable(_) => {
                let _warning = jackin_telemetry::record_recovered_degradation();
                AttachOutcome::still_running()
            }
        },
    )
}

pub async fn inspect_attach_outcome_by_id(
    docker: &impl DockerApi,
    container: &ContainerHandle,
) -> anyhow::Result<jackin_isolation::finalize::AttachOutcome> {
    use jackin_isolation::finalize::AttachOutcome;
    Ok(match docker.inspect_container_by_id(container).await {
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Created
        | ContainerState::Removing => AttachOutcome::still_running(),
        ContainerState::Dead => {
            let _warning = jackin_telemetry::record_recovered_degradation();
            AttachOutcome::still_running()
        }
        ContainerState::Stopped {
            oom_killed: true, ..
        } => AttachOutcome::oom_killed(),
        ContainerState::Stopped { exit_code, .. } => AttachOutcome::stopped(exit_code),
        ContainerState::NotFound | ContainerState::InspectUnavailable(_) => {
            let _warning = jackin_telemetry::record_recovered_degradation();
            AttachOutcome::still_running()
        }
    })
}
