// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Restore candidates: discovery, the launch-dialog choice, and
//! preserved-status persistence.
//!
//! Split out of `jackin-runtime` (S7 split 89); the old
//! `jackin_runtime::runtime::launch::restore::*` paths keep working through
//! the hub shim re-export.

use super::restore_resolve::RestoreResolution;
use jackin_core::JackinPaths;
use jackin_docker::docker_client::{ContainerState, DockerApi};
use jackin_instance::{InstanceIndex, InstanceManifest, InstanceQuery, InstanceStatus};
use jackin_runtime_launch_attach_outcome::attach_outcome::write_instance_status;
use jackin_runtime_launch_load_options::load_options::LoadOptions;

/// Build a `LaunchCandidate` for an `InstanceManifest` by reading its
/// isolation records and pre-fetching changed file content for D24 inspect.
///
/// `pub` (not `pub(crate)`) so the hub `launch::restore` suite — which stays
/// in `jackin-runtime` — keeps naming it through the hub shim re-export.
pub fn launch_candidate_for_manifest(
    paths: &JackinPaths,
    manifest: &InstanceManifest,
    label: impl FnOnce(&[jackin_core::IsolationRecord]) -> String,
) -> anyhow::Result<jackin_core::LaunchCandidate> {
    let state_dir = paths.data_dir.join(&manifest.container_base);
    let records =
        jackin_runtime_isolation::isolation::state::read_records(&state_dir).map_err(|error| {
            error.context(format!(
                "cannot establish restore isolation state for {}; refusing restore/delete choice",
                manifest.container_base
            ))
        })?;
    let label = label(&records);
    let is_dirty = records.iter().any(|r| {
        matches!(
            r.cleanup_status,
            jackin_core::CleanupStatus::PreservedDirty
                | jackin_core::CleanupStatus::PreservedUnpushed
        )
    });
    let inspect = records
        .iter()
        .map(|rec| {
            jackin_runtime_isolation::isolation::git_inspect::worktree_inspect(&rec.worktree_path)
        })
        .collect();
    Ok(jackin_core::LaunchCandidate {
        label,
        is_dirty,
        inspect,
    })
}

/// D23/D21: launch dialog with Del-to-delete and I-to-inspect (D24).
///
/// Builds `LaunchCandidate` objects from `candidates` + `related`, shows the
/// picker through the live launch surface or a standalone surface, and maps
/// the result back to `RestoreResolution`. `Delete(i)` returns
/// `PurgeAndRestartFresh` — the caller purges then restarts fresh.
pub fn present_restore_choice(
    progress: Option<&mut jackin_runtime_progress::progress::LaunchProgress>,
    paths: &JackinPaths,
    workspace_label: &str,
    role_key: &str,
    candidates: Vec<InstanceManifest>,
    related: &[RelatedRestoreCandidate],
) -> anyhow::Result<RestoreResolution> {
    // Build candidate list (same-role first, then related).
    let mut launch_candidates: Vec<jackin_core::LaunchCandidate> = candidates
        .iter()
        .map(|manifest| {
            launch_candidate_for_manifest(paths, manifest, |records| {
                restore_candidate_label_from_records(manifest, records)
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    launch_candidates.extend(
        related
            .iter()
            .map(|candidate| {
                launch_candidate_for_manifest(paths, &candidate.manifest, |records| {
                    format!(
                        "Recover other role with hardline {} docker:{}",
                        restore_candidate_label_from_records(&candidate.manifest, records),
                        candidate.docker_state.short_label()
                    )
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
    );

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
        jackin_core::LaunchDialogResult::StartFresh => {
            supersede_restore_candidates(paths, candidates)?;
            Ok(RestoreResolution::StartFresh)
        }
        jackin_core::LaunchDialogResult::Restore(i) if i < candidates.len() => Ok(
            RestoreResolution::RestoreCurrentRole(candidates[i].container_base.clone()),
        ),
        jackin_core::LaunchDialogResult::Restore(i) => {
            recover_related_restore_candidate(&related[i - candidates.len()])
        }
        jackin_core::LaunchDialogResult::Delete(i) => {
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
pub struct RelatedRestoreCandidate {
    pub manifest: InstanceManifest,
    pub docker_state: ContainerState,
}

pub async fn related_restore_candidates(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::Agent,
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
            .inspect_container_by_name(&manifest.container_base)
            .await
            .state;
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

pub fn recover_related_restore_candidate(
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
                jackin_runtime_attach_sessions::sessions::docker_unavailable_msg(
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

pub fn related_restore_load_options(
    current: &LoadOptions,
    manifest: &InstanceManifest,
) -> anyhow::Result<LoadOptions> {
    Ok(LoadOptions {
        debug: current.debug,
        rebuild: current.rebuild,
        force: current.force,
        host_env: current.host_env.clone(),
        entry_claim: current.entry_claim.clone(),
        agent: Some(manifest.agent()?),
        role_branch: manifest.role_source_ref.clone(),
        restore_container_base: Some(manifest.container_base.clone()),
        restore_role_source_git: Some(manifest.role_source_git.clone()),
        ..LoadOptions::default()
    })
}

/// Render the launch-dialog label for `manifest` (test seam shared with the
/// hub suite; un-gated at the split-89 move like split 87's `git_program`: a
/// `cfg(test)` fn would vanish from the leaf's non-test build that hub tests
/// link against).
#[expect(
    clippy::unwrap_used,
    reason = "un-gated test seam: unwrap inherited from hub cfg(test) code, reachable only from hub tests"
)]
pub fn restore_candidate_label(paths: &JackinPaths, manifest: &InstanceManifest) -> String {
    let state_dir = paths.data_dir.join(&manifest.container_base);
    let records = jackin_runtime_isolation::isolation::state::read_records(&state_dir).unwrap();
    restore_candidate_label_from_records(manifest, &records)
}

fn restore_candidate_label_from_records(
    manifest: &InstanceManifest,
    records: &[jackin_core::IsolationRecord],
) -> String {
    let isolation = jackin_runtime_isolation::isolation::state::MountSummary::from_records(records)
        .prompt_label();
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

pub fn supersede_restore_candidates(
    paths: &JackinPaths,
    candidates: Vec<InstanceManifest>,
) -> anyhow::Result<()> {
    for mut manifest in candidates {
        let state_dir = paths.data_dir.join(&manifest.container_base);
        write_instance_status(paths, &state_dir, &mut manifest, InstanceStatus::Superseded)?;
    }
    Ok(())
}

pub fn matching_instance_manifests(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::Agent,
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

pub fn matching_current_role_manifests(
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

/// Persist `Preserved`-tier status when `finalize_foreground_session`
/// decides to keep the isolation state. No-op for any other decision;
/// both the first finalize pass and the post-restart retry pass call
/// this so a future field added under the `Preserved` arm cannot drift
/// between them.
pub fn write_preserved_status_if_applicable(
    decision: jackin_runtime_isolation::isolation::finalize::FinalizeDecision,
    paths: &JackinPaths,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
) -> anyhow::Result<()> {
    if !matches!(
        decision,
        jackin_runtime_isolation::isolation::finalize::FinalizeDecision::Preserved
    ) {
        return Ok(());
    }
    let status = preserved_instance_status(state_dir)?;
    write_instance_status(paths, state_dir, manifest, status)
}

pub fn preserved_instance_status(state_dir: &std::path::Path) -> anyhow::Result<InstanceStatus> {
    use jackin_runtime_isolation::isolation::state::CleanupStatus;

    let records = jackin_runtime_isolation::isolation::state::read_records(state_dir)?;
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

pub fn manifest_host_workdir_fingerprint(workspace: &jackin_config::ResolvedWorkspace) -> String {
    workspace
        .mounts
        .iter()
        .filter(|mount| path_covers_workdir(&mount.dst, &workspace.workdir))
        .max_by_key(|mount| mount.dst.len())
        .map_or_else(
            || jackin_instance::manifest::host_path_fingerprint(&workspace.workdir),
            |mount| jackin_instance::manifest::host_path_fingerprint(&mount.src),
        )
}

fn path_covers_workdir(mount_dst: &str, workdir: &str) -> bool {
    let mount_dst = jackin_core::container_paths::normalize_path(std::path::Path::new(mount_dst));
    let workdir = jackin_core::container_paths::normalize_path(std::path::Path::new(workdir));
    jackin_core::container_paths::path_is_ancestor_or_equal(&mount_dst, &workdir)
}
