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
use super::{LaunchPlan, emit_launch_plan, emit_rejected_launch_plan};

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

#[allow(clippy::too_many_arguments)]
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
    let current = resolve_current_restore_candidate_timed(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        docker,
    )
    .await?;
    if let Some(current) = current {
        return Ok(current);
    }

    jackin_diagnostics::active_timing_started(
        "restore",
        "related_restore_candidates",
        Some(role_key),
    );
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
            jackin_diagnostics::active_timing_done(
                "restore",
                "related_restore_candidates",
                Some(&format!("{} candidates", related.len())),
            );
            related
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                "restore",
                "related_restore_candidates",
                Some("error"),
            );
            return Err(error);
        }
    };

    if related.is_empty() {
        emit_rejected_launch_plan(
            LaunchPlan::AttachExisting,
            "no_current_role_candidate",
            None,
            None,
        );
        emit_rejected_launch_plan(
            LaunchPlan::StartStopped,
            "no_current_role_candidate",
            None,
            None,
        );
        emit_rejected_launch_plan(
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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_current_restore_candidate_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
    jackin_diagnostics::active_timing_started(
        "restore",
        "current_restore_candidate",
        Some(role_key),
    );
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
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate",
                Some(detail),
            );
            Ok(current)
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate",
                Some("error"),
            );
            Err(error)
        }
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_unselected_current_restore_candidate_with_agent_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<UnselectedCurrentRestoreResolution>> {
    jackin_diagnostics::active_timing_started(
        "restore",
        "current_restore_candidate_unselected_agent",
        Some(role_key),
    );
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
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate_unselected_agent",
                Some(detail),
            );
            Ok(current)
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate_unselected_agent",
                Some("error"),
            );
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

#[allow(clippy::too_many_arguments)]
async fn resolve_unselected_current_restore_candidate_with_agent(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<UnselectedCurrentRestoreResolution>> {
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
        jackin_diagnostics::active_timing_started(
            "restore",
            "inspect_current_container",
            Some(&manifest.container_base),
        );
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        jackin_diagnostics::active_timing_done(
            "restore",
            "inspect_current_container",
            Some(docker_state.short_label().as_str()),
        );
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
                emit_rejected_launch_plan(
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
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    if multiple_candidates {
                        "current_role_agent_container_missing"
                    } else {
                        "single_current_role_agent_container_missing"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
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
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    if multiple_candidates {
                        "current_role_agent_container_not_attachable"
                    } else {
                        "single_current_role_agent_container_not_attachable"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
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
            emit_launch_plan(
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
                emit_rejected_launch_plan(
                    LaunchPlan::CreateFromValidImage,
                    "multiple_current_role_agents_need_selection",
                    None,
                    None,
                );
                Ok(None)
            }
        },
        _ => {
            emit_rejected_launch_plan(
                LaunchPlan::AttachExisting,
                "multiple_current_role_agents_need_selection",
                None,
                None,
            );
            emit_rejected_launch_plan(
                LaunchPlan::StartStopped,
                "multiple_current_role_agents_need_selection",
                None,
                None,
            );
            Ok(None)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_current_restore_candidate(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
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
        jackin_diagnostics::active_timing_started(
            "restore",
            "inspect_current_container",
            Some(&manifest.container_base),
        );
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        jackin_diagnostics::active_timing_done(
            "restore",
            "inspect_current_container",
            Some(docker_state.short_label().as_str()),
        );
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
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    "launch_never_reconnects_to_live_instance",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
            }
            ContainerState::Stopped { .. } | ContainerState::Created => {
                emit_launch_plan(
                    LaunchPlan::StartStopped,
                    "current_role_container_startable",
                    Some(&manifest.container_base),
                );
                return Ok(Some(RestoreResolution::StartCurrentRole(
                    manifest.container_base.clone(),
                )));
            }
            ContainerState::NotFound => {
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    "current_role_container_missing",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
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
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    "current_role_container_not_attachable",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
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
