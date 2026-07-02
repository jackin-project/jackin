//! Exit diagnosis helpers extracted from launch coordinator for premature exits,
//! attach failures, and outcome inspection.

use std::path::Path;

use jackin_core::CommandRunner;
use jackin_diagnostics;
use jackin_docker::docker_client::DockerApi;

use crate::runtime::ContainerState;

/// Whether `diagnose_premature_exit` is firing before the operator's
/// terminal was attached or after. The treatment of `exit 0` differs
/// between the two: pre-attach it's PID 1 exiting before the client
/// attaches (still worth surfacing — most likely a bad image or
/// missing binary), post-attach it's the multiplexer shutting the
/// container down because no live sessions remain (the
/// container-lifecycle-policy happy path — swallow it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitPhase {
    PreAttach,
    PostAttach,
}

/// inspect + log fetch so the surfaced error names the exit code, OOM
/// flag, and the last lines of the container's combined stdout/stderr.
///
/// Returns `None` when the container is still running (the normal
/// happy path) so the caller can proceed to the session exec.
pub(crate) async fn diagnose_premature_exit(
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container_name: &str,
    phase: ExitPhase,
    capsule_log_path: Option<&str>,
) -> Option<anyhow::Error> {
    let state = docker.inspect_container_state(container_name).await;
    diagnose_with_state(runner, container_name, &state, phase, capsule_log_path).await
}

/// Same diagnostic logic as `diagnose_premature_exit` but with the
/// inspected state passed in — callers that already inspected the
/// container can avoid a second `docker inspect` round-trip (and the
/// TOCTOU window between the two).
pub(crate) async fn diagnose_with_state(
    runner: &mut impl CommandRunner,
    container_name: &str,
    state: &ContainerState,
    phase: ExitPhase,
    capsule_log_path: Option<&str>,
) -> Option<anyhow::Error> {
    match state {
        // Default to letting the `docker exec` attempt proceed when state is
        // ambiguous: the daemon's own error from a true `NotFound`
        // (`No such container`) is just as actionable as anything we
        // could synthesize, and a transient inspect hiccup must not
        // hijack an otherwise-healthy launch.
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
            // Post-attach clean exit (exit 0, no OOM) is the normal
            // shutdown path: the operator typed `/exit` in the agent,
            // the multiplexer drained the last live session, and the
            // container shut itself down. The container-lifecycle
            // policy treats this as the happy path — return None so
            // the caller does not synthesize a misleading "exited
            // before attach" error. Pre-attach exit 0 is still
            // surfaced because PID 1 died before the
            // client connected indicates a bad image / missing binary
            // even when the exit code looks clean.
            if phase == ExitPhase::PostAttach && *exit_code == 0 && !oom_killed {
                return None;
            }
            // Distinguish "docker logs succeeded but was empty" from
            // "docker logs CLI failed" — the latter is a post-mortem
            // signal the operator needs (daemon down, container gone)
            // rather than the empty body the prose body falls back to.
            let logs = match runner
                .capture("docker", &["logs", "--tail", "40", container_name], None)
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
            } else if let Some(mux_tail) = capsule_log_path
                .map(Path::new)
                .and_then(|path| read_text_tail(path, 40).ok().flatten())
            {
                // `docker logs` is empty when the capsule daemon routes its
                // diagnostics to multiplexer.log rather than stderr. Surface
                // that file's tail so a pre-attach daemon crash is reported
                // with its real error instead of an opaque "no log output".
                format!(
                    "container {container_name} {phase_label} ({reason}); docker logs empty — last 40 multiplexer.log lines:\n{mux_tail}"
                )
            } else {
                format!(
                    "container {container_name} {phase_label} ({reason}) and produced no log output"
                )
            };
            // Emit a structured container exit event with the crash evidence so
            // the run JSONL is self-contained (Defect 41).
            if let Some(run) = jackin_diagnostics::active_run() {
                run.container_exited(
                    container_name,
                    (*exit_code).into(),
                    *oom_killed,
                    capsule_log_path.unwrap_or("(path unknown)"),
                    logs.as_deref(),
                );
            }
            Some(anyhow::anyhow!(body))
        }
    }
}

pub(crate) fn read_text_tail(path: &Path, max_lines: usize) -> anyhow::Result<Option<String>> {
    let lines = crate::runtime::logs::read_tail(path, max_lines)?;
    if lines.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lines.join("\n")))
    }
}

pub(crate) fn attach_failure_error(
    container_name: &str,
    err: &anyhow::Error,
    capsule_log_path: &Path,
    capsule_log_str: &str,
) -> anyhow::Error {
    let evidence = match read_text_tail(capsule_log_path, 40) {
        Ok(Some(tail)) => format!("last 40 capsule log lines:\n{tail}"),
        Ok(None) => format!("capsule log {capsule_log_str} had no output"),
        Err(error) => format!("failed to read capsule log {capsule_log_str}: {error:#}"),
    };
    anyhow::anyhow!(
        "capsule attach failed for {container_name}: {err}\ncapsule log: {capsule_log_str}\n{evidence}"
    )
}

/// Query a container's post-attach state for use by `finalize_foreground_session`.
///
/// Returns `AttachOutcome::still_running` when the container is still running
/// (terminal closed / detach), `AttachOutcome::oom_killed` when the kernel
/// killed the container OOM, otherwise `AttachOutcome::stopped(exit_code)`.
///
/// Capture failures (docker daemon hiccup, container removed mid-inspect)
/// are mapped to `still_running()` — the **conservative** default. Returning
/// `stopped(0)` here would route the call through `finalize_clean_exit`,
/// which combined with any concurrent git failure inside `assess_cleanup`
/// could auto-delete worktrees of containers that may actually still be
/// running. `still_running()` instead skips the auto-cleanup path entirely
/// and preserves records for `jackin hardline` to recover.
#[allow(clippy::unnecessary_wraps)] // Result preserved so callers' `?` keeps working without a churn-y signature change
pub(crate) async fn inspect_attach_outcome(
    docker: &impl DockerApi,
    container: &str,
) -> anyhow::Result<crate::isolation::finalize::AttachOutcome> {
    use crate::isolation::finalize::AttachOutcome;
    // Only `Stopped` with a clean or non-zero exit legitimately routes through
    // finalize_clean_exit. Paused/Restarting/Created/Removing are transient
    // active states — treating them as still_running is the conservative choice
    // that prevents finalize_clean_exit from auto-deleting worktrees of
    // containers that may resume. Dead is rare (daemon failed to deinitialize)
    // and also preserved for operator inspection.
    Ok(match docker.inspect_container_state(container).await {
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Created
        | ContainerState::Removing => AttachOutcome::still_running(),
        ContainerState::Dead => {
            jackin_diagnostics::debug_log!(
                "isolation",
                "inspect_attach_outcome: container {container} status=dead; treating as still_running to preserve records for inspection",
            );
            AttachOutcome::still_running()
        }
        ContainerState::Stopped {
            oom_killed: true, ..
        } => AttachOutcome::oom_killed(),
        ContainerState::Stopped { exit_code, .. } => AttachOutcome::stopped(exit_code),
        ContainerState::NotFound | ContainerState::InspectUnavailable(_) => {
            jackin_diagnostics::debug_log!(
                "isolation",
                "inspect_attach_outcome: docker inspect failed for {container}; treating as still_running (conservative — finalize_clean_exit's auto-cleanup never fires)",
            );
            AttachOutcome::still_running()
        }
    })
}
