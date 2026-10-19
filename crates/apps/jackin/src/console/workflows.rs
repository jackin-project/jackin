// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host executors for the Console Workflows screen. The screen emits
//! [`WorkflowsEffect`] intents; the host performs the daemon and
//! Capsule calls and feeds fresh snapshots back into the screen.
//!
//! Every daemon call goes through the local host socket; every
//! ownership call goes through the C2 session-control helpers (direct
//! socket with docker-exec fallback), the same path the CLI and the
//! scheduler use. Failures land in the screen error line; successes
//! clear it and reload the list plus the selected run detail.

use anyhow::{Context as _, Result};
use jackin_console::tui::screens::workflows::effect::WorkflowsEffect;
use jackin_console::tui::screens::workflows::model::{ObserveTarget, WorkflowScreen};

use super::adapter::state::{ManagerStage, ManagerState};

const LIST_LIMIT: u32 = 100;
const PAUSE_REASON_TAKEOVER: &str = "operator control from Console";
const CANCEL_REASON: &str = "operator cancel from Console";

fn workflows_screen_mut<'a>(state: &'a mut ManagerState<'_>) -> Option<&'a mut WorkflowScreen> {
    match &mut state.stage {
        ManagerStage::Workflows(screen) => Some(screen),
        _ => None,
    }
}

fn report_error(state: &mut ManagerState<'_>, error: anyhow::Error) -> bool {
    if let Some(screen) = workflows_screen_mut(state) {
        screen.set_error(Some(format!("{error:#}")));
    }
    true
}

/// Reload the run list plus the selected run detail into the screen.
/// Runs on every explicit navigation so the detail view always shows
/// the saved state for the selected run.
pub(crate) fn execute_workflows_refresh(
    state: &mut ManagerState<'_>,
    paths: &jackin_core::JackinPaths,
) -> bool {
    if !matches!(state.stage, ManagerStage::Workflows(_)) {
        return false;
    }
    let layout = jackin_runtime::host_daemon::DaemonLayout::new(paths);
    let runs = match request_run_list(&layout) {
        Ok(runs) => runs,
        Err(error) => return report_error(state, error),
    };
    let Some(screen) = workflows_screen_mut(state) else {
        return false;
    };
    screen.set_runs(runs);
    let selected = screen.selected_run().map(|run| run.run_id.clone());
    let Some(run_id) = selected else {
        screen.close_detail();
        screen.set_error(None);
        return true;
    };
    match request_run_detail(&layout, &run_id) {
        Ok(detail) => {
            let Some(screen) = workflows_screen_mut(state) else {
                return false;
            };
            screen.set_detail(detail);
            screen.set_error(None);
        }
        Err(error) => return report_error(state, error),
    }
    true
}

/// Perform one workflow intent from the Workflows screen.
pub(crate) fn execute_workflows_effect(
    state: &mut ManagerState<'_>,
    paths: &jackin_core::JackinPaths,
    effect: WorkflowsEffect,
) -> bool {
    if !matches!(state.stage, ManagerStage::Workflows(_)) {
        return false;
    }
    let result = match effect {
        WorkflowsEffect::Observe(target) => {
            observe_session(&target).and_then(|()| refresh_after(state, paths))
        }
        WorkflowsEffect::TakeControl(target) => take_over(state, paths, target),
        WorkflowsEffect::ReleaseControl(target) => release_control(state, paths, &target),
        WorkflowsEffect::Resume { run_id } => {
            resume_run(paths, &run_id).and_then(|()| refresh_after(state, paths))
        }
        WorkflowsEffect::Cancel { run_id } => {
            cancel_run(paths, &run_id).and_then(|()| refresh_after(state, paths))
        }
        WorkflowsEffect::Export { run_id, out_dir } => {
            export_run(paths, &run_id, &out_dir).and_then(|()| refresh_after(state, paths))
        }
    };
    match result {
        Ok(()) => true,
        Err(error) => report_error(state, error),
    }
}

fn refresh_after(state: &mut ManagerState<'_>, paths: &jackin_core::JackinPaths) -> Result<()> {
    execute_workflows_refresh(state, paths);
    // The refresh reports its own failure into the screen; surface it
    // here too so a successful mutation never looks silently stale.
    if let Some(screen) = workflows_screen_mut(state)
        && let Some(error) = screen.error()
    {
        anyhow::bail!("{error}");
    }
    Ok(())
}

/// Resolve the exact recorded session to a host container plus a
/// capsule session id. Both are required: the host never guesses a
/// session from a run id.
fn require_session_target(target: &ObserveTarget) -> Result<(String, u64)> {
    let instance = target
        .instance_id
        .clone()
        .filter(|instance| !instance.is_empty())
        .with_context(|| {
            format!(
                "run `{}` worker `{}` has no recorded instance to attach",
                target.run_id, target.worker_id
            )
        })?;
    let session = target
        .session_id
        .filter(|session| *session >= 0)
        .map(u64::try_from)
        .transpose()
        .with_context(|| {
            format!(
                "run `{}` worker `{}` recorded an out-of-range capsule session",
                target.run_id, target.worker_id
            )
        })?
        .with_context(|| {
            format!(
                "run `{}` worker `{}` has no recorded capsule session to attach",
                target.run_id, target.worker_id
            )
        })?;
    Ok((instance, session))
}

fn resolve_container(instance: &str) -> Result<jackin_core::ContainerHandle> {
    let request = jackin_process::ExecRequest::new(
        "docker",
        ["inspect", "--format", "{{.ID}}\t{{.Name}}", instance],
    );
    let output = crate::process_telemetry::exec_sync(&request)
        .context("resolving container identity for workflow attach")?;
    anyhow::ensure!(
        output.success,
        "docker inspect failed for workflow container {instance}"
    );
    let line = String::from_utf8_lossy(&output.stdout);
    let (id, raw_name) = line
        .lines()
        .map(str::trim)
        .find_map(|line| line.split_once('\t'))
        .with_context(|| format!("docker inspect returned no identity for {instance}"))?;
    let name = raw_name.trim_start_matches('/');
    anyhow::ensure!(
        name == instance,
        "docker inspect identity changed for workflow container {instance}: {name}"
    );
    jackin_core::ContainerHandle::new(name, id)
}

/// Confirm the exact recorded session is live without taking input
/// ownership: read-only `capsule status` over docker exec must list
/// the session id.
fn observe_session(target: &ObserveTarget) -> Result<()> {
    let (instance, session) = require_session_target(target)?;
    let container = resolve_container(&instance)?;
    let request = jackin_process::ExecRequest::new(
        "docker",
        [
            "exec",
            container.name(),
            jackin_core::container_paths::CAPSULE_BIN,
            "status",
        ],
    );
    let output = crate::process_telemetry::exec_sync(&request)
        .context("reading capsule status for workflow observe")?;
    anyhow::ensure!(
        output.success,
        "capsule status failed in workflow container {instance}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    anyhow::ensure!(
        stdout
            .lines()
            .any(|line| line.contains(&format!("[{session}]"))),
        "session {session} is not live in workflow container {instance}"
    );
    Ok(())
}

/// Take exclusive input ownership, mark the held session on the
/// screen, then pause the run while the operator holds it.
fn take_over(
    state: &mut ManagerState<'_>,
    paths: &jackin_core::JackinPaths,
    target: ObserveTarget,
) -> Result<()> {
    let (instance, session) = require_session_target(&target)?;
    let container = resolve_container(&instance)?;
    jackin_runtime_session_control::session_control::take_session_control(
        paths, &container, session,
    )
    .with_context(|| format!("taking control of session {session} in {instance}"))?;
    if let Some(screen) = workflows_screen_mut(state) {
        screen.set_observing(Some(target.clone()));
    }
    let layout = jackin_runtime::host_daemon::DaemonLayout::new(paths);
    if let Err(error) = request_pause(&layout, &target.run_id) {
        if let Some(screen) = workflows_screen_mut(state) {
            screen.set_error(Some(format!(
                "control of session {session} held, but pausing run `{}` failed: {error:#}",
                target.run_id
            )));
        }
        return Ok(());
    }
    refresh_after(state, paths)
}

/// Explicit release of operator control; the run stays paused until
/// the operator resumes it.
fn release_control(
    state: &mut ManagerState<'_>,
    paths: &jackin_core::JackinPaths,
    target: &ObserveTarget,
) -> Result<()> {
    let (instance, session) = require_session_target(target)?;
    let container = resolve_container(&instance)?;
    jackin_runtime_session_control::session_control::release_session_control(
        paths, &container, session,
    )
    .with_context(|| format!("releasing control of session {session} in {instance}"))?;
    if let Some(screen) = workflows_screen_mut(state) {
        screen.set_observing(None);
        screen.set_error(None);
    }
    refresh_after(state, paths)
}

fn resume_run(paths: &jackin_core::JackinPaths, run_id: &str) -> Result<()> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind, request};

    let layout = jackin_runtime::host_daemon::DaemonLayout::new(paths);
    match request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::WorkflowResume {
            run_id: run_id.to_owned(),
        },
    )
    .with_context(|| format!("resuming run `{run_id}`"))?
    .kind
    {
        DaemonResponseKind::WorkflowRun { .. } => Ok(()),
        DaemonResponseKind::Error { message, code } => {
            anyhow::bail!("cannot resume run `{run_id}`: [{code}] {message}")
        }
        other => {
            anyhow::bail!("cannot resume run `{run_id}`: unexpected daemon response {other:?}")
        }
    }
}

fn cancel_run(paths: &jackin_core::JackinPaths, run_id: &str) -> Result<()> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind, request};

    let layout = jackin_runtime::host_daemon::DaemonLayout::new(paths);
    match request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::WorkflowCancel {
            run_id: run_id.to_owned(),
            reason: CANCEL_REASON.to_owned(),
        },
    )
    .with_context(|| format!("cancelling run `{run_id}`"))?
    .kind
    {
        DaemonResponseKind::WorkflowRun { .. } => Ok(()),
        DaemonResponseKind::Error { message, code } => {
            anyhow::bail!("cannot cancel run `{run_id}`: [{code}] {message}")
        }
        other => {
            anyhow::bail!("cannot cancel run `{run_id}`: unexpected daemon response {other:?}")
        }
    }
}

fn export_run(paths: &jackin_core::JackinPaths, run_id: &str, out_dir: &str) -> Result<()> {
    let layout = jackin_runtime::host_daemon::DaemonLayout::new(paths);
    let detail = request_run_detail_events(&layout, run_id)?;
    crate::app::workflow_export::write_report(&detail, None, std::path::Path::new(out_dir))
        .with_context(|| format!("exporting run `{run_id}` to {out_dir}"))?;
    Ok(())
}

fn request_run_list(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
) -> Result<Vec<jackin_workflow::RunSummary>> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind, request};

    match request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::WorkflowList { limit: LIST_LIMIT },
    )
    .context("listing workflow runs")?
    .kind
    {
        DaemonResponseKind::WorkflowRuns { summaries } => Ok(summaries),
        DaemonResponseKind::Error { message, code } => {
            anyhow::bail!("cannot list workflow runs: [{code}] {message}")
        }
        other => anyhow::bail!("cannot list workflow runs: unexpected daemon response {other:?}"),
    }
}

fn request_run_detail(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    run_id: &str,
) -> Result<jackin_workflow::RunDetail> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind, request};

    match request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::WorkflowDetail {
            run_id: run_id.to_owned(),
            after_seq: -1,
            event_limit: 0,
        },
    )
    .with_context(|| format!("reading run `{run_id}` detail"))?
    .kind
    {
        DaemonResponseKind::WorkflowRunDetail { detail } => Ok(detail),
        DaemonResponseKind::Error { message, code } => {
            anyhow::bail!("cannot read run `{run_id}` detail: [{code}] {message}")
        }
        other => {
            anyhow::bail!("cannot read run `{run_id}` detail: unexpected daemon response {other:?}")
        }
    }
}

fn request_run_detail_events(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    run_id: &str,
) -> Result<jackin_workflow::RunDetail> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind, request};

    match request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::WorkflowDetail {
            run_id: run_id.to_owned(),
            after_seq: -1,
            event_limit: 500,
        },
    )
    .with_context(|| format!("reading run `{run_id}` export detail"))?
    .kind
    {
        DaemonResponseKind::WorkflowRunDetail { detail } => Ok(detail),
        DaemonResponseKind::Error { message, code } => {
            anyhow::bail!("cannot export run `{run_id}`: [{code}] {message}")
        }
        other => {
            anyhow::bail!("cannot export run `{run_id}`: unexpected daemon response {other:?}")
        }
    }
}

fn request_pause(layout: &jackin_runtime::host_daemon::DaemonLayout, run_id: &str) -> Result<()> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind, request};

    match request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::WorkflowPause {
            run_id: run_id.to_owned(),
            reason: PAUSE_REASON_TAKEOVER.to_owned(),
            checkpoint: false,
        },
    )
    .with_context(|| format!("pausing run `{run_id}` for operator control"))?
    .kind
    {
        DaemonResponseKind::WorkflowRun { .. } => Ok(()),
        DaemonResponseKind::Error { message, code } => {
            anyhow::bail!("cannot pause run `{run_id}`: [{code}] {message}")
        }
        other => anyhow::bail!("cannot pause run `{run_id}`: unexpected daemon response {other:?}"),
    }
}

#[cfg(test)]
mod tests;
