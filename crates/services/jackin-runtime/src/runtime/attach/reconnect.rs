// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Reconnect-or-create session flows with focus and lease variants.

use anyhow::Context as _;
use jackin_core::container_paths;
use jackin_core::{CommandRunner, ContainerHandle, RunOptions};
use jackin_docker::docker_client::DockerApi;

use jackin_core::JackinPaths;

use super::{
    host_alt_screen_exec_flag, insert_run_as_user, mark_reconnect_admission_failure,
    require_current_account_admission, set_role_terminal_title,
    start_or_reconnect_capsule_client_with_handle_with_lease, validate_current_account_admission,
    validate_recorded_role_handle, wait_for_capsule_daemon_with_handle,
};

pub(crate) async fn reconnect_or_create_session_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let admission_lease = require_current_account_admission(paths, container_name)
        .map_err(mark_reconnect_admission_failure)?;
    reconnect_or_create_session_with_focus_with_lease(
        paths,
        container_name,
        focus_session,
        &admission_lease,
        docker,
        runner,
    )
    .await
}

pub(crate) async fn reconnect_or_create_session_with_focus_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)
        .map_err(mark_reconnect_admission_failure)?;
    let inspection = docker.inspect_container_by_name(container_name).await;
    let container = inspection
        .handle
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot resolve container {container_name}: {}",
                inspection.state.inspect_label()
            )
        })
        .map_err(mark_reconnect_admission_failure)?;
    validate_recorded_role_handle(paths, container_name, &container)
        .map_err(mark_reconnect_admission_failure)?;
    reconnect_or_create_session_with_container_handle_with_lease(
        paths,
        container_name,
        focus_session,
        admission_lease,
        docker,
        runner,
        &container,
        None,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "restore threads exact Docker ownership, account revision, and pending entry leases"
)]
pub(crate) async fn reconnect_or_create_session_with_container_handle_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
    entry_claim: Option<&crate::runtime::universe::EntryClaim>,
) -> anyhow::Result<()> {
    validate_recorded_role_handle(paths, container_name, container)
        .map_err(mark_reconnect_admission_failure)?;
    set_role_terminal_title(paths, container_name);
    wait_for_capsule_daemon_with_handle(paths, container, docker)
        .await
        .map_err(mark_reconnect_admission_failure)?;
    admission_lease
        .ensure_current(paths)
        .map_err(mark_reconnect_admission_failure)?;
    if let Some(claim) = entry_claim {
        claim
            .activate()
            .await
            .context("activating running launch entry")
            .map_err(mark_reconnect_admission_failure)?;
    }
    if crate::runtime::host_attach::host_attach_enabled(paths) {
        let outcome = crate::runtime::host_attach::run_host_attach_session(
            paths,
            container,
            None,
            focus_session,
            &[],
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        admission_lease
            .ensure_current(paths)
            .map_err(mark_reconnect_admission_failure)?;
        return outcome;
    }
    let focus_arg = focus_session.map(|id| id.to_string());
    let run_as_user = Some(crate::runtime::identity::CAPSULE_SUPERVISOR_USER);
    let mut args: Vec<&str> = vec!["exec", "-it", container.id(), container_paths::CAPSULE_BIN];
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    insert_run_as_user(&mut args, run_as_user);
    if let Some(ref id) = focus_arg {
        args.push("--focus");
        args.push(id);
    }
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "capsule_client_exec",
        Some(container_name),
    );
    admission_lease
        .ensure_current(paths)
        .map_err(mark_reconnect_admission_failure)?;
    let outcome = runner
        .run(
            "docker",
            &args,
            None,
            &RunOptions {
                interactive: true,
                ..RunOptions::default()
            },
        )
        .await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "capsule_client_exec",
        if outcome.is_ok() {
            Some("detached")
        } else {
            Some("error")
        },
    );
    if outcome.is_ok()
        && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            jackin_telemetry::schema::events::CAPSULE_SESSION_DETACH,
            "operator detached from capsule session",
        );
    }
    // The capsule has detached; re-claim the alt screen before any post-attach
    // work so the exit flow does not flash the operator's shell.
    jackin_diagnostics::reassert_alt_screen();
    admission_lease
        .ensure_current(paths)
        .map_err(mark_reconnect_admission_failure)?;
    outcome
}

pub(crate) async fn start_or_reconnect_capsule_client(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let admission_lease = require_current_account_admission(paths, container_name)?;
    start_or_reconnect_capsule_client_with_lease(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
    )
    .await
}

pub(crate) async fn start_or_reconnect_capsule_client_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    start_or_reconnect_capsule_client_with_handle_with_lease(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        None,
        None,
    )
    .await
    .map(|_| ())
}
