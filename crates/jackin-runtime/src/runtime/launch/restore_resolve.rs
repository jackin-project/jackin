#![allow(clippy::too_many_lines, reason = "documented residual allow; prefer expect when site is lint-true")]
//! Restore candidate resolution logic extracted from the launch coordinator
//! (the ~508L cluster of resolve_* fns + Restore* types). All public items
//! re-exported from the parent launch coordinator to preserve `super::` call
//! sites in `launch_pipeline.rs` and any `use super::*` in tests.

use crate::instance::InstanceManifest;
use crate::runtime::attach::ContainerState;

use jackin_core::paths::JackinPaths;
use jackin_docker::docker_client::DockerApi;

use super::restore::{
    matching_current_role_manifests, matching_instance_manifests, present_restore_choice,
    related_restore_candidates,
};
use super::{LaunchPlan, emit_launch_plan_for_run, emit_rejected_launch_plan_for_run};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RestoreResolution {
    StartFresh,
    StartCurrentRole(String),
    RecreateCurrentRole(String),
    RestoreCurrentRole(String),
    RecoverRelatedRole(String),
    RebuildRelatedRole(Box<InstanceManifest>),
    /// D21: operator deleted this instance from the launch dialog.
    /// Caller must purge the state dir then proceed as `StartFresh`.
    PurgeAndRestartFresh(String),
}

/// Outcome of the early current-role restore scan performed before role-repo
/// work (launch-speed 008c). When the final selected agent matches the scan
/// scope, the later `resolve_restore_candidate` reuses this and skips a second
/// current-role Docker inspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EarlyCurrentRestoreScan {
    /// Early scan was skipped (rebuild / pinned restore base / role branch).
    NotRun,
    /// Current-role candidates were scanned for a concrete agent (selected or
    /// the sole unselected agent). `None` means no attach/start/recreate hit.
    Scanned {
        agent: jackin_core::agent::Agent,
        /// Stashed outcome for that agent. When present, later resolve reuses
        /// the typed hit without a second Docker inspect; when `None`, later
        /// resolve skips current-role inspect entirely for this agent.
        current: Option<RestoreResolution>,
    },
    /// Unselected early scan proved the role has no current-role restore
    /// candidates under [`InstanceManifest::is_restore_candidate`] (broader
    /// than the launch-dialog filter). Any later selected agent may skip
    /// current-role re-inspect — agent-scoped matching would also be empty.
    ScannedUnselectedEmpty,
}

/// True when the early scan already proved there is no current-role candidate
/// for `agent`, so a second Docker inspect would be pure waste.
#[cfg(test)]
pub(crate) fn early_scan_skips_current_inspect(
    early: &EarlyCurrentRestoreScan,
    agent: jackin_core::agent::Agent,
) -> bool {
    matches!(early_scan_reused_current(early, agent), Some(None))
}

/// When the early scan can fully answer the current-role question for `agent`,
/// returns `Some(cached)` (`None` = no candidate, `Some(r)` = reuse hit).
/// `None` means the caller must re-run the Docker inspect path.
pub(crate) fn early_scan_reused_current(
    early: &EarlyCurrentRestoreScan,
    agent: jackin_core::agent::Agent,
) -> Option<Option<RestoreResolution>> {
    match early {
        EarlyCurrentRestoreScan::NotRun => None,
        EarlyCurrentRestoreScan::ScannedUnselectedEmpty => Some(None),
        EarlyCurrentRestoreScan::Scanned {
            agent: scanned_agent,
            current,
        } if *scanned_agent == agent => Some(current.clone()),
        EarlyCurrentRestoreScan::Scanned { .. } => None,
    }
}

/// Full resolve without early-scan reuse (tests and callers that did not run
/// the pre-role-repo current-role scan).
#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
#[allow(dead_code, reason = "documented residual allow; prefer expect when site is lint-true")] // re-exported for tests; production uses reusing_early
pub(crate) async fn resolve_restore_candidate(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
    progress: Option<&mut crate::runtime::progress::LaunchProgress>,
) -> anyhow::Result<RestoreResolution> {
    resolve_restore_candidate_reusing_early(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        docker,
        progress,
        &EarlyCurrentRestoreScan::NotRun,
    )
    .await
}

/// Like [`resolve_restore_candidate`], but reuses an early current-role scan
/// when the final agent matches so the common path does not re-inspect.
#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
pub(crate) async fn resolve_restore_candidate_reusing_early(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
    progress: Option<&mut crate::runtime::progress::LaunchProgress>,
    early: &EarlyCurrentRestoreScan,
) -> anyhow::Result<RestoreResolution> {
    let current = match early_scan_reused_current(early, agent) {
        // Reuse typed empty or non-empty early hit (skip second inspect).
        Some(cached) => cached,
        None => {
            resolve_current_restore_candidate_timed(
                paths,
                workspace_name,
                workspace_label,
                workdir,
                role_key,
                agent,
                docker,
            )
            .await?
        }
    };
    if let Some(current) = current {
        return Ok(current);
    }

    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    if let Some(run) = &active_run {
        run.timing_started("restore", "related_restore_candidates", Some(role_key));
    }
    let related_result = related_restore_candidates(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        docker,
    )
    .await;
    let related = match related_result {
        Ok(related) => {
            if let Some(run) = &active_run {
                run.timing_done(
                    "restore",
                    "related_restore_candidates",
                    Some(&format!("{} candidates", related.len())),
                );
            }
            related
        }
        Err(error) => {
            if let Some(run) = &active_run {
                run.timing_done("restore", "related_restore_candidates", Some("error"));
            }
            return Err(error);
        }
    };

    if related.is_empty() {
        emit_rejected_launch_plan_scoped(
            active_run.as_deref(),
            LaunchPlan::AttachExisting,
            "no_current_role_candidate",
            None,
            None,
        );
        emit_rejected_launch_plan_scoped(
            active_run.as_deref(),
            LaunchPlan::StartStopped,
            "no_current_role_candidate",
            None,
            None,
        );
        emit_rejected_launch_plan_scoped(
            active_run.as_deref(),
            LaunchPlan::CreateFromValidImage,
            "no_current_role_candidate",
            None,
            None,
        );
        return Ok(RestoreResolution::StartFresh);
    }

    // Related stale-state decisions still require an explicit rich prompt so
    // launching one role never silently recovers or supersedes another role.
    present_restore_choice(
        progress,
        paths,
        workspace_label,
        role_key,
        Vec::new(),
        &related,
    )
}

#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
pub(crate) async fn resolve_current_restore_candidate_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    if let Some(run) = &active_run {
        run.timing_started("restore", "current_restore_candidate", Some(role_key));
    }
    let result = resolve_current_restore_candidate(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        docker,
    )
    .await;
    match result {
        Ok(current) => {
            let detail = current
                .as_ref()
                .map_or("none", current_restore_timing_detail);
            if let Some(run) = &active_run {
                run.timing_done("restore", "current_restore_candidate", Some(detail));
            }
            Ok(current)
        }
        Err(error) => {
            if let Some(run) = &active_run {
                run.timing_done("restore", "current_restore_candidate", Some("error"));
            }
            Err(error)
        }
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
pub(crate) async fn resolve_unselected_current_restore_candidate_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
    Ok(
        resolve_unselected_current_restore_candidate_with_agent_timed(
            paths,
            workspace_name,
            workspace_label,
            workdir,
            role_key,
            docker,
        )
        .await?
        .map(|candidate| candidate.resolution),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnselectedCurrentRestoreResolution {
    pub resolution: RestoreResolution,
    pub agent: jackin_core::agent::Agent,
}

#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
pub(crate) async fn resolve_unselected_current_restore_candidate_with_agent_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<UnselectedCurrentRestoreResolution>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    if let Some(run) = &active_run {
        run.timing_started(
            "restore",
            "current_restore_candidate_unselected_agent",
            Some(role_key),
        );
    }
    let result = resolve_unselected_current_restore_candidate_with_agent(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        docker,
    )
    .await;
    match result {
        Ok(current) => {
            let detail = current.as_ref().map_or("none", |candidate| {
                current_restore_timing_detail(&candidate.resolution)
            });
            if let Some(run) = &active_run {
                run.timing_done(
                    "restore",
                    "current_restore_candidate_unselected_agent",
                    Some(detail),
                );
            }
            Ok(current)
        }
        Err(error) => {
            if let Some(run) = &active_run {
                run.timing_done(
                    "restore",
                    "current_restore_candidate_unselected_agent",
                    Some("error"),
                );
            }
            Err(error)
        }
    }
}

fn current_restore_timing_detail(resolution: &RestoreResolution) -> &'static str {
    match resolution {
        RestoreResolution::StartCurrentRole(_) => "start_stopped",
        RestoreResolution::RecreateCurrentRole(_) => "create_from_valid_image",
        _ => "other",
    }
}

fn emit_rejected_launch_plan_scoped(
    run: Option<&jackin_diagnostics::RunDiagnostics>,
    plan: LaunchPlan,
    reason: &str,
    container: Option<&str>,
    state: Option<&str>,
) {
    if let Some(run) = run {
        emit_rejected_launch_plan_for_run(run, plan, reason, container, state);
    }
}

fn emit_launch_plan_scoped(
    run: Option<&jackin_diagnostics::RunDiagnostics>,
    plan: LaunchPlan,
    reason: &str,
    container: Option<&str>,
) {
    if let Some(run) = run {
        emit_launch_plan_for_run(run, plan, reason, container);
    }
}

#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
async fn resolve_unselected_current_restore_candidate_with_agent(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<UnselectedCurrentRestoreResolution>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    // D10: launch dialog shows only un-cleanly-terminated instances; live
    // containers (Active/Running) are excluded because D13 means the launch
    // path never re-attaches to a live instance.
    let candidates =
        matching_current_role_manifests(paths, workspace_name, workspace_label, workdir, role_key)?
            .into_iter()
            .filter(InstanceManifest::is_launch_restore_candidate)
            .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Ok(None);
    }

    let multiple_candidates = candidates.len() > 1;
    let mut runnable = Vec::new();
    let mut recreatable = Vec::new();
    for manifest in candidates {
        let agent = manifest.agent()?;
        if let Some(run) = &active_run {
            run.timing_started(
                "restore",
                "inspect_current_container",
                Some(&manifest.container_base),
            );
        }
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        if let Some(run) = &active_run {
            run.timing_done(
                "restore",
                "inspect_current_container",
                Some(docker_state.short_label().as_str()),
            );
        }
        if let ContainerState::InspectUnavailable(reason) = docker_state {
            anyhow::bail!(
                "{}",
                crate::runtime::attach::docker_unavailable_msg(
                    &format!(
                        "inspect matching jackin instance `{}`",
                        manifest.container_base
                    ),
                    &reason,
                )
            );
        }
        match docker_state {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                // D13: launch never reconnects to a live instance (ADR 0001).
                // Running instances are reachable from the console via explicit
                // instance selection (hardline); the launch path always creates
                // a new container or restores an un-cleanly-terminated one.
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::AttachExisting,
                    "launch_never_reconnects_to_live_instance",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
            }
            ContainerState::Stopped { .. } | ContainerState::Created => {
                runnable.push(UnselectedCurrentRestoreResolution {
                    resolution: RestoreResolution::StartCurrentRole(manifest.container_base),
                    agent,
                });
            }
            ContainerState::NotFound => {
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::AttachExisting,
                    if multiple_candidates {
                        "current_role_agent_container_missing"
                    } else {
                        "single_current_role_agent_container_missing"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::StartStopped,
                    if multiple_candidates {
                        "current_role_agent_container_missing"
                    } else {
                        "single_current_role_agent_container_missing"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                recreatable.push(UnselectedCurrentRestoreResolution {
                    resolution: RestoreResolution::RecreateCurrentRole(manifest.container_base),
                    agent,
                });
            }
            ContainerState::Removing
            | ContainerState::Dead
            | ContainerState::InspectUnavailable(_) => {
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::AttachExisting,
                    if multiple_candidates {
                        "current_role_agent_container_not_attachable"
                    } else {
                        "single_current_role_agent_container_not_attachable"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::StartStopped,
                    if multiple_candidates {
                        "current_role_agent_container_not_startable"
                    } else {
                        "single_current_role_agent_container_not_startable"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
            }
        }
    }

    match runnable.as_slice() {
        [
            UnselectedCurrentRestoreResolution {
                resolution: RestoreResolution::StartCurrentRole(container),
                agent,
            },
        ] => {
            emit_launch_plan_scoped(
                active_run.as_deref(),
                LaunchPlan::StartStopped,
                if multiple_candidates {
                    "only_viable_current_role_agent_container_startable"
                } else {
                    "single_current_role_agent_container_startable"
                },
                Some(container),
            );
            Ok(Some(UnselectedCurrentRestoreResolution {
                resolution: RestoreResolution::StartCurrentRole(container.clone()),
                agent: *agent,
            }))
        }
        [] => match recreatable.as_slice() {
            [candidate] => Ok(Some(candidate.clone())),
            [] => Ok(None),
            _ => {
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::CreateFromValidImage,
                    "multiple_current_role_agents_need_selection",
                    None,
                    None,
                );
                Ok(None)
            }
        },
        _ => {
            emit_rejected_launch_plan_scoped(
                active_run.as_deref(),
                LaunchPlan::AttachExisting,
                "multiple_current_role_agents_need_selection",
                None,
                None,
            );
            emit_rejected_launch_plan_scoped(
                active_run.as_deref(),
                LaunchPlan::StartStopped,
                "multiple_current_role_agents_need_selection",
                None,
                None,
            );
            Ok(None)
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
pub(crate) async fn resolve_current_restore_candidate(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    for manifest in matching_instance_manifests(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
    )? {
        if !manifest.is_restore_candidate() {
            continue;
        }
        if let Some(run) = &active_run {
            run.timing_started(
                "restore",
                "inspect_current_container",
                Some(&manifest.container_base),
            );
        }
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        if let Some(run) = &active_run {
            run.timing_done(
                "restore",
                "inspect_current_container",
                Some(docker_state.short_label().as_str()),
            );
        }
        if let ContainerState::InspectUnavailable(reason) = docker_state {
            anyhow::bail!(
                "{}",
                crate::runtime::attach::docker_unavailable_msg(
                    &format!(
                        "inspect matching jackin instance `{}`",
                        manifest.container_base
                    ),
                    &reason,
                )
            );
        }
        match docker_state {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                // D13: launch never reconnects to a live instance (ADR 0001).
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::AttachExisting,
                    "launch_never_reconnects_to_live_instance",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
            }
            ContainerState::Stopped { .. } | ContainerState::Created => {
                emit_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::StartStopped,
                    "current_role_container_startable",
                    Some(&manifest.container_base),
                );
                return Ok(Some(RestoreResolution::StartCurrentRole(
                    manifest.container_base.clone(),
                )));
            }
            ContainerState::NotFound => {
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::AttachExisting,
                    "current_role_container_missing",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::StartStopped,
                    "current_role_container_missing",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                return Ok(Some(RestoreResolution::RecreateCurrentRole(
                    manifest.container_base.clone(),
                )));
            }
            ContainerState::Removing
            | ContainerState::Dead
            | ContainerState::InspectUnavailable(_) => {
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::AttachExisting,
                    "current_role_container_not_attachable",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan_scoped(
                    active_run.as_deref(),
                    LaunchPlan::StartStopped,
                    "current_role_container_not_startable",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
            }
        }
    }
    Ok(None)
}
