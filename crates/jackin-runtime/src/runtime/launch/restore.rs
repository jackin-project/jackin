use super::{LoadOptions, RestoreResolution};
use crate::instance::{InstanceIndex, InstanceManifest, InstanceQuery, InstanceStatus};
use crate::runtime::attach::ContainerState;
use jackin_core::paths::JackinPaths;
use jackin_docker::docker_client::DockerApi;
use std::path::PathBuf;

#[cfg(test)]
mod tests;

/// Build a `LaunchCandidate` for an `InstanceManifest` by reading its
/// isolation records and pre-fetching changed file content for D24 inspect.
fn launch_candidate_for_manifest(
    paths: &JackinPaths,
    manifest: &InstanceManifest,
    label: String,
) -> jackin_launch::LaunchCandidate {
    let state_dir = paths.data_dir.join(&manifest.container_base);
    let records = crate::isolation::state::read_records(&state_dir).unwrap_or_default();
    let is_dirty = records.iter().any(|r| {
        matches!(
            r.cleanup_status,
            jackin_core::isolation_record::CleanupStatus::PreservedDirty
                | jackin_core::isolation_record::CleanupStatus::PreservedUnpushed
        )
    });
    let inspect = records
        .iter()
        .map(|rec| crate::isolation::git_inspect::worktree_inspect(&rec.worktree_path))
        .collect();
    jackin_launch::LaunchCandidate {
        label,
        is_dirty,
        inspect,
    }
}

/// D23/D21: launch dialog with Del-to-delete and I-to-inspect (D24).
///
/// Builds `LaunchCandidate` objects from `candidates` + `related`, shows the
/// picker through the live launch surface or a standalone surface, and maps
/// the result back to `RestoreResolution`. `Delete(i)` returns
/// `PurgeAndRestartFresh` — the caller purges then restarts fresh.
pub(super) fn present_restore_choice(
    progress: Option<&mut crate::runtime::progress::LaunchProgress>,
    paths: &JackinPaths,
    workspace_label: &str,
    role_key: &str,
    candidates: Vec<InstanceManifest>,
    related: &[RelatedRestoreCandidate],
) -> anyhow::Result<RestoreResolution> {
    // Build candidate list (same-role first, then related).
    let mut launch_candidates: Vec<jackin_launch::LaunchCandidate> = candidates
        .iter()
        .map(|manifest| {
            launch_candidate_for_manifest(paths, manifest, restore_candidate_label(paths, manifest))
        })
        .collect();
    launch_candidates.extend(related.iter().map(|candidate| {
        let label = format!(
            "Recover other role with hardline {}",
            related_restore_candidate_label(paths, candidate)
        );
        launch_candidate_for_manifest(paths, &candidate.manifest, label)
    }));

    let Some(progress) = progress else {
        let hint = candidates.first().map_or_else(
            || format!("role `{role_key}`"),
            |manifest| format!("`jackin hardline {}`", manifest.container_base),
        );
        anyhow::bail!(
            "unfinished jackin instances exist for workspace `{workspace_label}` and role `{role_key}` but the rich launch dialog is unavailable; run {hint} to inspect or recover, or purge stale instances before a fresh load"
        );
    };

    let result =
        progress.launch_dialog_progress("Unfinished jackin instances", &launch_candidates)?;

    match result {
        jackin_launch::LaunchDialogResult::StartFresh => {
            supersede_restore_candidates(paths, candidates)?;
            Ok(RestoreResolution::StartFresh)
        }
        jackin_launch::LaunchDialogResult::Restore(i) if i < candidates.len() => Ok(
            RestoreResolution::RestoreCurrentRole(candidates[i].container_base.clone()),
        ),
        jackin_launch::LaunchDialogResult::Restore(i) => {
            recover_related_restore_candidate(&related[i - candidates.len()])
        }
        jackin_launch::LaunchDialogResult::Delete(i) => {
            let container = if i < candidates.len() {
                candidates[i].container_base.clone()
            } else {
                related[i - candidates.len()]
                    .manifest
                    .container_base
                    .clone()
            };
            Ok(RestoreResolution::PurgeAndRestartFresh(container))
        }
    }
}

#[derive(Debug)]
pub(super) struct RelatedRestoreCandidate {
    pub(super) manifest: InstanceManifest,
    pub(super) docker_state: ContainerState,
}

pub(super) async fn related_restore_candidates(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
) -> anyhow::Result<Vec<RelatedRestoreCandidate>> {
    let mut candidates = Vec::new();
    for manifest in InstanceIndex::matching_manifests(
        &paths.data_dir,
        InstanceQuery {
            workspace_name,
            workspace_label,
            workdir,
            role_key: None,
            agent_runtime: None,
        },
    )? {
        if manifest.role_key == role_key && manifest.agent_runtime == agent.slug() {
            continue;
        }
        if !manifest.is_restore_candidate() {
            continue;
        }
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        let should_prompt = match docker_state {
            ContainerState::InspectUnavailable(_) | ContainerState::NotFound => true,
            ContainerState::Running
            | ContainerState::Paused
            | ContainerState::Restarting
            | ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => false,
        };
        if should_prompt {
            candidates.push(RelatedRestoreCandidate {
                manifest,
                docker_state,
            });
        }
    }
    Ok(candidates)
}

pub(super) fn recover_related_restore_candidate(
    candidate: &RelatedRestoreCandidate,
) -> anyhow::Result<RestoreResolution> {
    match candidate.docker_state {
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Stopped { .. } => Ok(RestoreResolution::RecoverRelatedRole(
            candidate.manifest.container_base.clone(),
        )),
        ContainerState::NotFound
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => Ok(RestoreResolution::RebuildRelatedRole(Box::new(
            candidate.manifest.clone(),
        ))),
        ContainerState::InspectUnavailable(ref reason) => {
            anyhow::bail!(
                "{}",
                crate::runtime::attach::docker_unavailable_msg(
                    &format!(
                        "inspect related jackin instance `{}`",
                        candidate.manifest.container_base
                    ),
                    reason,
                )
            );
        }
    }
}

pub(super) fn related_restore_load_options(
    current: &LoadOptions,
    manifest: &InstanceManifest,
) -> anyhow::Result<LoadOptions> {
    Ok(LoadOptions {
        debug: current.debug,
        rebuild: current.rebuild,
        force: current.force,
        host_env: current.host_env.clone(),
        agent: Some(manifest.agent()?),
        role_branch: manifest.role_source_ref.clone(),
        restore_container_base: Some(manifest.container_base.clone()),
        restore_role_source_git: Some(manifest.role_source_git.clone()),
        ..LoadOptions::default()
    })
}

pub(super) fn related_restore_candidate_label(
    paths: &JackinPaths,
    candidate: &RelatedRestoreCandidate,
) -> String {
    format!(
        "{} docker:{}",
        restore_candidate_label(paths, &candidate.manifest),
        candidate.docker_state.short_label()
    )
}

pub(super) fn restore_candidate_label(paths: &JackinPaths, manifest: &InstanceManifest) -> String {
    let state_dir = paths.data_dir.join(&manifest.container_base);
    let isolation = crate::isolation::state::MountSummary::prompt_label_for_state_dir(&state_dir);
    let attach = manifest
        .last_attach_outcome
        .as_deref()
        .map_or_else(String::new, |outcome| format!(" attach:{outcome}"));
    format!(
        "{} status:{} agent:{} role:{} updated:{} {}{}",
        manifest.instance_id,
        manifest.status.label(),
        manifest.agent_runtime,
        manifest.role_key,
        manifest.updated_at,
        isolation,
        attach
    )
}

pub(super) fn supersede_restore_candidates(
    paths: &JackinPaths,
    candidates: Vec<InstanceManifest>,
) -> anyhow::Result<()> {
    for mut manifest in candidates {
        let state_dir = paths.data_dir.join(&manifest.container_base);
        write_instance_status(paths, &state_dir, &mut manifest, InstanceStatus::Superseded)?;
    }
    Ok(())
}

pub(super) fn matching_instance_manifests(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
) -> anyhow::Result<Vec<InstanceManifest>> {
    InstanceIndex::matching_manifests(
        &paths.data_dir,
        InstanceQuery {
            workspace_name,
            workspace_label,
            workdir,
            role_key: Some(role_key),
            agent_runtime: Some(agent),
        },
    )
}

pub(super) fn matching_current_role_manifests(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
) -> anyhow::Result<Vec<InstanceManifest>> {
    InstanceIndex::matching_manifests(
        &paths.data_dir,
        InstanceQuery {
            workspace_name,
            workspace_label,
            workdir,
            role_key: Some(role_key),
            agent_runtime: None,
        },
    )
}

pub(in crate::runtime) fn write_instance_status(
    paths: &JackinPaths,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
    status: InstanceStatus,
) -> anyhow::Result<()> {
    manifest.mark_status(status);
    manifest.write(state_dir)?;
    InstanceIndex::update_manifest(&paths.data_dir, manifest)?;
    Ok(())
}

pub(super) fn write_instance_attach_outcome(
    paths: &JackinPaths,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
    outcome: crate::isolation::finalize::AttachOutcome,
) -> anyhow::Result<()> {
    if matches!(
        outcome,
        crate::isolation::finalize::AttachOutcome::StillRunning
    ) {
        manifest.mark_status(InstanceStatus::Running);
    } else {
        manifest.touch();
    }
    manifest.last_attach_outcome = Some(format_attach_outcome(outcome));
    manifest.write(state_dir)?;
    InstanceIndex::update_manifest(&paths.data_dir, manifest)?;
    Ok(())
}

pub(in crate::runtime) fn record_instance_attach_outcome(
    paths: &JackinPaths,
    container_name: &str,
    outcome: crate::isolation::finalize::AttachOutcome,
) -> anyhow::Result<()> {
    let state_dir = paths.data_dir.join(container_name);
    // Missing manifest is a legitimate no-op; corrupt manifest is
    // logged so the attach-outcome record is not silently dropped.
    let Some(mut manifest) =
        InstanceManifest::read_or_log(&state_dir, "record_instance_attach_outcome")
    else {
        return Ok(());
    };
    write_instance_attach_outcome(paths, &state_dir, &mut manifest, outcome)
}

pub(super) fn format_attach_outcome(outcome: crate::isolation::finalize::AttachOutcome) -> String {
    use crate::isolation::finalize::AttachOutcome;
    match outcome {
        AttachOutcome::OomKilled => "oom_killed".to_owned(),
        AttachOutcome::StillRunning => "running".to_owned(),
        AttachOutcome::Stopped(code) => format!("exit:{code}"),
    }
}

/// Persist `Preserved`-tier status when `finalize_foreground_session`
/// decides to keep the isolation state. No-op for any other decision;
/// both the first finalize pass and the post-restart retry pass call
/// this so a future field added under the `Preserved` arm cannot drift
/// between them.
pub(super) fn write_preserved_status_if_applicable(
    decision: crate::isolation::finalize::FinalizeDecision,
    paths: &JackinPaths,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
) -> anyhow::Result<()> {
    if !matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::Preserved
    ) {
        return Ok(());
    }
    let status = preserved_instance_status(state_dir)?;
    write_instance_status(paths, state_dir, manifest, status)
}

pub(in crate::runtime) fn preserved_instance_status(
    state_dir: &std::path::Path,
) -> anyhow::Result<InstanceStatus> {
    use crate::isolation::state::CleanupStatus;

    let records = crate::isolation::state::read_records(state_dir)?;
    if records
        .iter()
        .any(|record| record.cleanup_status == CleanupStatus::PreservedDirty)
    {
        return Ok(InstanceStatus::PreservedDirty);
    }
    if records
        .iter()
        .any(|record| record.cleanup_status == CleanupStatus::PreservedUnpushed)
    {
        return Ok(InstanceStatus::PreservedUnpushed);
    }
    Ok(InstanceStatus::RestoreAvailable)
}

pub(super) fn manifest_host_workdir_fingerprint(
    workspace: &jackin_config::ResolvedWorkspace,
) -> String {
    workspace
        .mounts
        .iter()
        .filter(|mount| path_covers_workdir(&mount.dst, &workspace.workdir))
        .max_by_key(|mount| mount.dst.len())
        .map_or_else(
            || crate::instance::manifest::host_path_fingerprint(&workspace.workdir),
            |mount| crate::instance::manifest::host_path_fingerprint(&mount.src),
        )
}

/// Host path of the capsule's `multiplexer.log` for a given container.
///
/// Layout: `<data_dir>/<container_name>/state/multiplexer.log`, matching
/// the bind-mount declared in `agent_mounts`.
pub(super) fn capsule_multiplexer_log_path(paths: &JackinPaths, container_name: &str) -> PathBuf {
    paths
        .data_dir
        .join(container_name)
        .join("state")
        .join("multiplexer.log")
}

fn path_covers_workdir(mount_dst: &str, workdir: &str) -> bool {
    let mount_dst = mount_dst.trim_end_matches('/');
    workdir == mount_dst
        || workdir
            .strip_prefix(mount_dst)
            .is_some_and(|suffix| suffix.starts_with('/'))
}
