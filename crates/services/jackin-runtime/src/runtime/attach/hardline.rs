// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Hardline agent flows with focus and lease handling.

use jackin_core::{CommandRunner, ContainerHandle};
use jackin_docker::docker_client::DockerApi;

use jackin_core::JackinPaths;

use super::{
    ContainerState, ReconnectAdmissionFailure, finalize_reconnected_foreground_session_with_handle,
    inspect_unavailable_message, mark_reconnect_admission_failure, missing_restore_message,
    reconnect_or_create_session_with_container_handle_with_lease,
    require_current_account_admission, validate_current_account_admission,
    validate_recorded_role_handle,
};

pub async fn hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    hardline_agent_with_focus(paths, container_name, None, docker, runner).await
}

/// Same as `hardline_agent` but threads a host-supplied pane focus id.
///
/// The console preview navigation calls this with the
/// operator-selected pane so the reconnect lands inside that pane.
pub async fn hardline_agent_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    use crate::runtime::backend::ContainerBackend as _;

    match crate::runtime::backend::backend_for_state(paths, container_name) {
        crate::runtime::backend::InstanceBackend::Docker => {
            crate::runtime::backend::DockerBackend::new(docker)
                .hardline(paths, container_name, focus_session, runner)
                .await
        }
        crate::runtime::backend::InstanceBackend::AppleContainer => {
            crate::runtime::backend::AppleContainerBackend::production()
                .hardline(paths, container_name, focus_session, runner)
                .await
        }
    }
}

pub(crate) async fn hardline_docker_agent_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "hardline_container_inspect",
        Some(container_name),
    );
    let inspection = docker.inspect_container_by_name(container_name).await;
    let container_state = inspection.state;
    let container_handle = inspection.handle;
    let container_state_label = container_state.short_label();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "hardline_container_inspect",
        Some(&container_state_label),
    );
    match container_state {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            let Some(container) = container_handle else {
                anyhow::bail!("container '{container_name}' inspection returned no immutable ID");
            };
            validate_recorded_role_handle(paths, container_name, &container)?;
            let admission_lease = require_current_account_admission(paths, container_name)?;
            hardline_docker_agent_with_focus_with_lease(
                paths,
                container_name,
                focus_session,
                &admission_lease,
                docker,
                runner,
                &container,
                None,
            )
            .await
        }
        ContainerState::NotFound => {
            if let Some(message) = missing_restore_message(paths, container_name)? {
                anyhow::bail!("{message}");
            }
            anyhow::bail!(
                "container '{container_name}' not found; use `jackin load` to start a new session"
            )
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!("{}", inspect_unavailable_message(container_name, &reason))
        }
        ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        } => {
            anyhow::bail!(
                "container '{container_name}' exited cleanly; \
                 use `jackin load` to start a new session"
            )
        }
        ContainerState::Stopped {
            exit_code,
            oom_killed,
        } => {
            let reason = if oom_killed {
                "OOM killed".to_owned()
            } else {
                format!("exit {exit_code}")
            };
            anyhow::bail!(
                "container '{container_name}' stopped ({reason}); \
                 use `jackin load` to start a new session or recover saved state"
            )
        }
        state @ (ContainerState::Created | ContainerState::Removing | ContainerState::Dead) => {
            anyhow::bail!(
                "container '{container_name}' is not running (state: {}); \
                use `jackin load` to start a new session",
                state.short_label()
            )
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "restore threads exact Docker ownership, account revision, and pending entry leases"
)]
pub(crate) async fn hardline_docker_agent_with_focus_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
    entry_claim: Option<&crate::runtime::universe::EntryClaim>,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)
        .map_err(mark_reconnect_admission_failure)?;
    // Reconcile keep_awake right before reconnect. The attach blocks on the
    // capsule exec until the session ends, so the post-hardline reconcile in
    // the app layer would fire too late.
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    let attach_outcome = reconnect_or_create_session_with_container_handle_with_lease(
        paths,
        container_name,
        focus_session,
        admission_lease,
        docker,
        runner,
        container,
        entry_claim,
    )
    .await;
    // A clean last-session shutdown surfaces as a non-zero attach result (the
    // capsule client hits the socket close as `early eof`). Preserve that one
    // known transport race, but never swallow admission or generation errors
    // while the lifecycle inspect still says the container is running.
    if let Err(error) = attach_outcome {
        if error.is::<crate::runtime::launch::GenerationLeaseViolation>()
            || error.is::<ReconnectAdmissionFailure>()
        {
            return Err(error);
        }
        let inspect = docker.inspect_container_by_id(container).await;
        if let Some(diag) = crate::runtime::launch::diagnose_with_state_by_id(
            runner,
            container,
            &inspect,
            crate::runtime::launch::ExitPhase::PostAttach,
        )
        .await
        {
            return Err(diag);
        }
        if !crate::runtime::launch::is_known_socket_close(&error, &inspect) {
            return Err(error);
        }
        let _warning = jackin_telemetry::record_recovered_degradation();
    }

    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        container,
    )
    .await
}
