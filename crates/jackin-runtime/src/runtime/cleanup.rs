// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container and class teardown: purge role-state directories, remove Docker
//! resources (containers, images, networks, volumes), and update the instance
//! index to reflect the deletion.
//!
//! Drives each filesystem teardown to completion before batching index
//! updates — if an early deletion fails, already-deleted entries are still
//! recorded so the index stays consistent with disk state.

#![expect(
    clippy::print_stderr,
    reason = "runtime cleanup and GC report operator-visible warnings and results"
)]

use anyhow::Context as _;
use super::prune_output;
use crate::instance::{DockerResources, InstanceIndex, InstanceManifest, InstanceStatus};
use crate::isolation::safe_remove::OwnedRemoval;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_core::{CommandRunner, ContainerHandle, NetworkId};
use jackin_docker::docker_client::{ContainerState, DockerApi, RemoveImageOutcome};
use owo_colors::OwoColorize;

use super::backend::{ContainerBackend as _, InstanceBackend};
use super::discovery::{list_managed_role_names, list_role_names};
use super::naming::{
    LABEL_IMAGE_KEY, LABEL_KIND_DIND, LABEL_KIND_PREWARM_DIND, LABEL_KIND_ROLE, LABEL_MANAGED,
    LABEL_ROLE_KEY,
};

struct CleanupTiming {
    name: &'static str,
}

impl Drop for CleanupTiming {
    fn drop(&mut self) {
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Cleanup,
            self.name,
            None,
        );
    }
}

fn cleanup_timing(name: &'static str) -> CleanupTiming {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Cleanup,
        name,
        None,
    );
    CleanupTiming { name }
}

fn cleanup_failure(_message: impl AsRef<str>) {
    let _error =
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
}

fn read_cleanup_manifest(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<InstanceManifest> {
    let mut components = std::path::Path::new(container_name).components();
    anyhow::ensure!(
        matches!(components.next(), Some(std::path::Component::Normal(name)) if name == std::ffi::OsStr::new(container_name))
            && components.next().is_none(),
        "invalid instance directory name; refusing cleanup"
    );
    let state_dir = paths.data_dir.join(container_name);
    let metadata_dir = state_dir.join(".jackin");
    for directory in [&paths.data_dir, &state_dir, &metadata_dir] {
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) => anyhow::ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "aliased instance directory at {}; refusing cleanup",
                directory.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let manifest_path = state_dir.join(".jackin/instance.json");
    match std::fs::symlink_metadata(&manifest_path) {
        Ok(metadata) => anyhow::ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "aliased instance manifest at {}; refusing cleanup",
            manifest_path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let manifest = InstanceManifest::read_optional(&state_dir)?.ok_or_else(|| {
        anyhow::anyhow!(
            "instance ownership manifest unavailable for {container_name}; retaining custody"
        )
    })?;
    anyhow::ensure!(
        manifest.container_base == container_name
            && manifest.docker.role_container == container_name,
        "instance ownership directory differs from manifest"
    );
    RoleSelector::parse(&manifest.role_key).map_err(|error| {
        anyhow::Error::new(error).context(format!(
            "invalid persisted role identity at {}; refusing cleanup",
            state_dir.display()
        ))
    })?;
    anyhow::ensure!(
        !matches!(
            super::backend::backend_for_manifest(Some(&manifest)),
            InstanceBackend::AppleContainer
        ) || manifest.docker_identity.is_none(),
        "contradictory Apple and Docker ownership identity; retaining custody"
    );
    Ok(manifest)
}

pub async fn purge_class_data(
    paths: &JackinPaths,
    selector: &RoleSelector,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("class_data");

    // Drive each filesystem teardown to completion, then batch the
    // index update for whichever containers succeeded. Returning early
    // on the first failure without recording the prior successes would
    // leave the index claiming the already-deleted state dirs still
    // hold their pre-purge status.
    let inventory = cleanup_instance_inventory(paths)?;
    let mut plans = Vec::new();
    for manifest in inventory {
        if RoleSelector::parse(&manifest.role_key)? == *selector {
            let name = manifest.container_base;
            let plan = admit_container_purge(paths, &name, docker).await?;
            plans.push((name, plan));
        }
    }
    let mut matched = Vec::new();
    let mut retained = Vec::new();
    for (file_name, plan) in plans {
        match apply_container_purge(plan, runner).await {
            Ok(()) => matched.push(file_name),
            Err(error) => {
                cleanup_failure(format!("class data purge failed: {error}"));
                retained.push(RetainedInstanceState {
                    container_base: file_name,
                    cause: error,
                });
            }
        }
    }
    let refs: Vec<&str> = matched.iter().map(String::as_str).collect();
    if let Err(cause) = InstanceIndex::mark_many_purged(&paths.data_dir, &refs) {
        retained.push(RetainedInstanceState {
            container_base: "instance index".to_owned(),
            cause,
        });
    }
    if retained.is_empty() {
        Ok(())
    } else {
        Err(InstanceCleanupFailures { retained }.into())
    }
}

pub async fn purge_container_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("container_state");
    purge_container_filesystem(paths, container_name, docker, runner).await?;
    InstanceIndex::mark_purged(&paths.data_dir, container_name)
}

/// Per-container filesystem teardown (docker-state guard + isolation
/// cleanup + state directory removal). Index updates are batched by the
/// caller so multi-container purges avoid an O(M²) read-rewrite cycle.
async fn purge_container_filesystem(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("container_filesystem");
    let plan = admit_container_purge(paths, container_name, docker).await?;
    apply_container_purge(plan, runner).await
}

struct ContainerPurgePlan {
    paths: JackinPaths,
    state_dir: std::path::PathBuf,
    state: OwnedRemoval,
    socket: OwnedRemoval,
    shared_lifetime: Option<crate::instance::SharedDockerLifetime>,
}

async fn admit_container_purge(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<ContainerPurgePlan> {
    let state = admit_container_state_removal(paths, container_name).await?;
    admit_container_purge_with_state(paths, container_name, docker, state).await
}

async fn admit_container_state_removal(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<OwnedRemoval> {
    let state_dir = paths.data_dir.join(container_name);
    super::coordination::ensure_prunable_async(paths, &state_dir).await?;
    let root = paths.data_dir.clone();
    let admitted_dir = state_dir.clone();
    jackin_telemetry::spawn::joined_blocking(move || {
        OwnedRemoval::admit_contained(&root, &admitted_dir)
    })
    .await
    .map_err(std::io::Error::other)?
    .map_err(anyhow::Error::from)
}

async fn admit_container_purge_with_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    state: OwnedRemoval,
) -> anyhow::Result<ContainerPurgePlan> {
    let state_dir = paths.data_dir.join(container_name);
    let manifest = read_cleanup_manifest(paths, container_name)?;
    ensure_backend_absent_for_purge(paths, container_name, docker).await?;
    crate::isolation::state::read_records(&state_dir)?;
    let shared_lifetime = if matches!(
        super::backend::backend_for_manifest(Some(&manifest)),
        InstanceBackend::Docker
    ) {
        let shared =
            resolve_shared_cleanup_for_state(paths, container_name, &manifest.docker, docker)
                .await?;
        anyhow::ensure!(
            shared.network.is_none() && shared.volume.is_none(),
            "shared Docker resources still exist for {container_name}; retaining instance custody"
        );
        ensure_shared_plan_current(paths, &shared, docker).await?;
        Some(shared.lifetime)
    } else { None };
    let socket = admit_socket_removal(paths, container_name).await?;
    Ok(ContainerPurgePlan {
        paths: paths.clone(),
        state_dir,
        state,
        socket,
        shared_lifetime,
    })
}

async fn apply_container_purge(
    mut plan: ContainerPurgePlan,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    if let Some(lifetime) = &plan.shared_lifetime {
        if !lifetime.is_retired() {
            let daemon = crate::instance::SharedDockerLifetime::load_for_cleanup(
                &plan.paths,
                lifetime.daemon_server_id(),
                lifetime.owner(),
            )?;
            anyhow::ensure!(daemon.as_ref() == Some(lifetime), "shared Docker custody changed before local purge");
            plan.shared_lifetime = Some(lifetime.begin_retirement(&plan.paths)?);
        }
    }
    crate::isolation::cleanup::purge_isolated_for_container(&plan.state_dir, runner).await?;
    // Owned-validated-path removal: the container name is operator/index
    // input, so deletion is containment-bound to the data dir and fd-pinned
    // (`O_NOFOLLOW` at every level). Escapes and symlinks are refused
    // loudly instead of followed; a missing dir is still a no-op.
    // Remove the host-side bind-mount dir (~/.jackin/sockets/<container>/)
    // that holds the daemon socket and Capsule launch config. Skipping it
    // here leaks stale `agent.toml` across load/purge cycles; a future
    // launch with the same container basename would bind-mount the old
    // contents before the host's mkdir + write overwrites them.
    remove_admitted_socket(plan.socket).await?;
    jackin_telemetry::spawn::joined_blocking(move || plan.state.remove())
        .await
        .map_err(std::io::Error::other)??;
    if let Some(lifetime) = plan.shared_lifetime {
        if !lifetime.is_retired() {
            lifetime.retire(&plan.paths)?;
        }
    }
    // Coordination inodes live outside the purged runtime state and persist.
    Ok(())
}

pub async fn eject_role(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("eject_role");
    match super::backend::backend_for_state(paths, container_name) {
        InstanceBackend::Docker => eject_docker_role(paths, container_name, docker).await,
        InstanceBackend::AppleContainer => {
            super::backend::AppleContainerBackend::production()
                .eject(paths, container_name)
                .await
        }
    }
}

pub(crate) async fn eject_docker_role(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let (resources, role, dind) =
        resolve_cleanup_handles_for_state(paths, container_name, None, docker).await?;
    let role = role.ok_or_else(|| anyhow::anyhow!("container {container_name} is not present"))?;
    eject_docker_role_with_resources(
        paths,
        container_name,
        docker,
        &role,
        dind.as_ref(),
        &resources,
    )
    .await
}

/// Eject a Docker role using identities captured before any destructive
/// operation. A persisted sidecar name is lookup context only: if its
/// immutable identity is unavailable, this function aborts before removing
/// either container rather than risking a same-name replacement.
pub(crate) async fn eject_docker_role_with_handles(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    role_handle: &ContainerHandle,
    dind_handle: Option<&ContainerHandle>,
) -> anyhow::Result<()> {
    let manifest = read_cleanup_manifest(paths, container_name)?;
    {
        let identity = manifest.docker_identity.as_ref().ok_or_else(|| anyhow::anyhow!(
            "Docker ownership identity unavailable for {container_name}; refusing destructive cleanup"
        ))?;
        anyhow::ensure!(
            role_handle.id() == identity.role_container_id,
            "role container ownership identity mismatch for {container_name}"
        );
        if let Some(handle) = dind_handle {
            anyhow::ensure!(
                Some(handle.id()) == identity.dind_container_id.as_deref(),
                "DinD container ownership identity mismatch for {}",
                handle.name()
            );
        }
    }
    let resources = manifest.docker;
    eject_docker_role_with_resources(
        paths,
        container_name,
        docker,
        role_handle,
        dind_handle,
        &resources,
    )
    .await
}

async fn eject_docker_role_with_resources(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    role_handle: &ContainerHandle,
    dind_handle: Option<&ContainerHandle>,
    resources: &DockerResources,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        role_handle.name() == container_name,
        "role container handle name mismatch: expected {container_name}, got {}",
        role_handle.name()
    );
    if let Some(dind_container) = resources.dind_container.as_deref() {
        let Some(dind_handle) = dind_handle else {
            anyhow::bail!(
                "DinD container identity unavailable; aborting destructive cleanup for {dind_container}"
            );
        };
        anyhow::ensure!(
            dind_handle.name() == dind_container,
            "DinD container handle name mismatch: expected {dind_container}, got {}",
            dind_handle.name()
        );
    }
    crate::isolation::state::read_records(&paths.data_dir.join(container_name))?;
    let mut shared = resolve_shared_cleanup_for_state(paths, container_name, resources, docker).await?;
    anyhow::ensure!(shared.role.as_ref().is_none_or(|handle| handle.id() == role_handle.id()),
        "role container differs from durable shared lifetime for {container_name}");
    anyhow::ensure!(shared.dind.as_ref().map(ContainerHandle::id) == dind_handle.map(ContainerHandle::id),
        "DinD container differs from durable shared lifetime for {container_name}");
    let socket = admit_socket_removal(paths, container_name).await?;
    ensure_shared_plan_current(paths, &shared, docker).await?;
    shared.lifetime = shared.lifetime.begin_retirement(paths)?;
    ensure_shared_plan_current(paths, &shared, docker).await?;

    // Remove containers first so the network has no active endpoints.
    docker.remove_container_by_id(role_handle).await?;
    if resources.dind_container.is_some() {
        // The prevalidated handle is the only permitted destructive target.
        let dind_handle = dind_handle.ok_or_else(|| {
            anyhow::anyhow!("DinD container identity disappeared before destructive cleanup")
        })?;
        docker.remove_container_by_id(dind_handle).await?;
    }

    // Volume and network are independent of each other once containers are gone.
    if let Some(certs_volume) = shared.volume.as_deref() {
        docker.remove_volume(certs_volume).await?;
        anyhow::ensure!(docker.inspect_volume_by_name(certs_volume).await?.is_none(),
            "certificate volume {certs_volume} remains after removal; retaining shared custody");
    }
    if let Some(network) = shared.network {
        docker.remove_network_by_id(&network).await?;
    }

    // Host-side socket dir cleanup. Same reason as
    // purge_container_filesystem above: the daemon socket and the
    // bind-mounted Capsule launch config live under
    // ~/.jackin/sockets/<container>/ and must be removed alongside the
    // docker-side teardown so re-launching the same container basename
    // does not inherit stale state.
    remove_admitted_socket(socket).await?;
    shared.lifetime.retire(paths)?;
    Ok(())
}

/// One immutable authority snapshot for the complete destructive preflight.
pub(crate) async fn resolve_cleanup_handles_for_state(
    paths: &JackinPaths,
    container_name: &str,
    known_role: Option<&ContainerHandle>,
    docker: &impl DockerApi,
) -> anyhow::Result<(
    DockerResources,
    Option<ContainerHandle>,
    Option<ContainerHandle>,
)> {
    let manifest = read_cleanup_manifest(paths, container_name)?;
    let resources = manifest.docker.clone();
    let shared = resolve_shared_cleanup_for_state(paths, container_name, &resources, docker).await?;
    if let Some(known) = known_role {
        anyhow::ensure!(
            known.name() == container_name
                && shared.role.as_ref().is_some_and(|captured| captured.id() == known.id()),
            "role container ownership identity mismatch before destructive cleanup for {container_name}"
        );
    }
    Ok((resources, shared.role, shared.dind))
}

/// Resolve only identities belonging to the recorded launch. A name lookup
/// checks absence/replacement; it can never establish ownership.
pub(crate) async fn resolve_role_handle_for_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<ContainerHandle> {
    resolve_optional_role_handle_for_state(paths, container_name, docker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("container {container_name} is not present"))
}

pub(crate) async fn resolve_optional_role_handle_for_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<ContainerHandle>> {
    let manifest = read_cleanup_manifest(paths, container_name)?;
    let shared = resolve_shared_cleanup_for_state(paths, container_name, &manifest.docker, docker).await?;
    Ok(shared.role)
}

pub(crate) async fn resolve_dind_handle_for_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<ContainerHandle>> {
    let manifest = read_cleanup_manifest(paths, container_name)?;
    let shared = resolve_shared_cleanup_for_state(paths, container_name, &manifest.docker, docker).await?;
    Ok(shared.dind)
}

pub(crate) fn docker_resources_for_state(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<DockerResources> {
    let manifest = read_cleanup_manifest(paths, container_name)?;
    Ok(manifest.docker)
}

pub(crate) struct SharedCleanupPlan {
    pub(crate) lifetime: crate::instance::SharedDockerLifetime,
    pub(crate) role: Option<ContainerHandle>,
    pub(crate) dind: Option<ContainerHandle>,
    pub(crate) network: Option<NetworkId>,
    pub(crate) volume: Option<String>,
}

pub(crate) async fn reconcile_shared_lifetime(
    paths: &JackinPaths,
    mut lifetime: crate::instance::SharedDockerLifetime,
    docker: &impl DockerApi,
) -> anyhow::Result<SharedCleanupPlan> {
    use crate::instance::{SharedCertsVolumeCustody, SharedNetworkCustody};

    let daemon = docker.daemon_server_id().await?;
    anyhow::ensure!(&daemon == lifetime.daemon_server_id(),
        "Docker daemon differs from shared lifetime; retaining generation {}", lifetime.generation());
    let rows = docker.list_containers(&[], true).await
        .context("listing containers to reconcile shared lifetime custody")?;
    let mut generation_rows = Vec::new();
    for row in &rows {
        if row.labels.get("jackin.shared-generation").map(String::as_str) == Some(lifetime.generation()) {
            generation_rows.push(row);
        }
    }
    let role = reconcile_lifetime_container(&mut lifetime, false, &rows, docker, paths).await?;
    let dind = reconcile_lifetime_container(&mut lifetime, true, &rows, docker, paths).await?;
    for row in generation_rows {
        let expected = [role.as_ref(), dind.as_ref()].into_iter().flatten()
            .any(|handle| handle.id() == row.id && handle.name() == row.name);
        anyhow::ensure!(expected,
            "unknown container {} carries shared generation {}; retaining custody",
            row.name, lifetime.generation());
    }

    let network_rows = docker.list_networks(&[]).await
        .context("listing networks to reconcile shared lifetime custody")?;
    let network = match lifetime.network().clone() {
        SharedNetworkCustody::Disabled => None,
        SharedNetworkCustody::Pending { name } => {
            let candidates = network_rows.iter().filter(|row| {
                row.name == name
                    || row.labels.get("jackin.shared-generation").map(String::as_str) == Some(lifetime.generation())
            }).collect::<Vec<_>>();
            match candidates.as_slice() {
                [] => None,
                [row] => {
                    validate_shared_network(row, &lifetime, &name)?;
                    lifetime.capture_network(row.id.clone())?;
                    lifetime.save(paths)?;
                    Some(row.id.clone())
                }
                _ => anyhow::bail!("multiple networks claim shared generation {}; retaining custody", lifetime.generation()),
            }
        }
        SharedNetworkCustody::Owned { name, id } => {
            let candidates = network_rows.iter().filter(|row| {
                row.id == id || row.name == name
                    || row.labels.get("jackin.shared-generation").map(String::as_str) == Some(lifetime.generation())
            }).collect::<Vec<_>>();
            match candidates.as_slice() {
                [] => None,
                [row] => {
                    validate_shared_network(row, &lifetime, &name)?;
                    anyhow::ensure!(row.id == id, "shared network ID changed for generation {}", lifetime.generation());
                    Some(id)
                }
                _ => anyhow::bail!("multiple networks claim shared generation {}; retaining custody", lifetime.generation()),
            }
        }
    };

    let volume = match lifetime.certs_volume().clone() {
        SharedCertsVolumeCustody::Disabled => None,
        SharedCertsVolumeCustody::Pending { name } | SharedCertsVolumeCustody::Owned { name } => {
            match docker.inspect_volume_by_name(&name).await? {
                None => None,
                Some(row) => {
                    let labels = shared_resource_labels(&lifetime);
                    anyhow::ensure!(row.name == name && row.driver == "local" && row.labels == labels,
                        "certificate volume ownership differs from shared generation {}; retaining custody", lifetime.generation());
                    if matches!(lifetime.certs_volume(), SharedCertsVolumeCustody::Pending { .. }) {
                        lifetime.capture_certs_volume()?;
                        lifetime.save(paths)?;
                    }
                    Some(name)
                }
            }
        }
    };
    let final_daemon = docker.daemon_server_id().await?;
    anyhow::ensure!(final_daemon == daemon, "Docker daemon identity changed during shared lifetime reconciliation");
    if lifetime.is_retired() {
        anyhow::ensure!(role.is_none() && dind.is_none() && network.is_none() && volume.is_none(),
            "retired shared generation {} has a Docker resource present; retaining local state", lifetime.generation());
    }
    Ok(SharedCleanupPlan { lifetime, role, dind, network, volume })
}

async fn reconcile_lifetime_container(
    lifetime: &mut crate::instance::SharedDockerLifetime,
    dind: bool,
    rows: &[jackin_core::ContainerRow],
    docker: &impl DockerApi,
    paths: &JackinPaths,
) -> anyhow::Result<Option<ContainerHandle>> {
    use crate::instance::{SharedContainerCustody, SharedDockerOwnerKind};
    let custody = if dind { lifetime.dind_container().clone() } else { lifetime.role_container().clone() };
    let (name, captured_id) = match custody {
        SharedContainerCustody::Disabled => return Ok(None),
        SharedContainerCustody::Pending { name } => (name, None),
        SharedContainerCustody::Owned { name, id } => (name, Some(id)),
    };
    let kind = if dind {
        if lifetime.namespace_owner_kind() == SharedDockerOwnerKind::Prewarm { "prewarm-dind" } else { "dind" }
    } else { "role" };
    let mut labels = shared_resource_labels(lifetime);
    labels.insert("jackin.kind".to_owned(), kind.to_owned());
    if dind && lifetime.namespace_owner_kind() == SharedDockerOwnerKind::Role {
        labels.insert("jackin.role".to_owned(), lifetime.namespace_owner().to_owned());
    }
    if dind && lifetime.namespace_owner_kind() == SharedDockerOwnerKind::Prewarm {
        labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    }
    let candidates = rows.iter().filter(|row| {
        row.name == name
            || captured_id.as_deref() == Some(row.id.as_str())
            || (row.labels.get("jackin.shared-generation").map(String::as_str) == Some(lifetime.generation())
                && row.labels.get("jackin.kind").map(String::as_str) == Some(kind))
    }).collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Ok(None),
        [row] => {
            anyhow::ensure!(row.name == name, "shared container name changed for generation {}", lifetime.generation());
            anyhow::ensure!(labels.iter().all(|(key, value)| row.labels.get(key) == Some(value)),
                "shared container labels differ for generation {}; retaining custody", lifetime.generation());
            if let Some(expected_id) = captured_id.as_deref() {
                anyhow::ensure!(row.id == expected_id, "shared container ID changed for generation {}; retaining custody", lifetime.generation());
            } else {
                lifetime.capture_container(dind, &row.id)?;
                lifetime.save(paths)?;
            }
            let handle = row.handle().context("shared container has invalid immutable ID")?;
            match docker.inspect_container_by_id(&handle).await {
                ContainerState::InspectUnavailable(reason) => anyhow::bail!("cannot verify shared container identity: {reason}"),
                ContainerState::NotFound => anyhow::bail!("shared container disappeared during custody reconciliation"),
                _ => Ok(Some(handle)),
            }
        }
        _ => anyhow::bail!("multiple containers claim shared generation {}; retaining custody", lifetime.generation()),
    }
}

fn shared_resource_labels(
    lifetime: &crate::instance::SharedDockerLifetime,
) -> std::collections::HashMap<String, String> {
    std::collections::HashMap::from([
        ("jackin.shared-generation".to_owned(), lifetime.generation().to_owned()),
        ("jackin.shared-owner".to_owned(), lifetime.namespace_owner().to_owned()),
        ("jackin.managed".to_owned(), "true".to_owned()),
    ])
}

fn validate_shared_network(
    row: &jackin_core::NetworkRow,
    lifetime: &crate::instance::SharedDockerLifetime,
    expected_name: &str,
) -> anyhow::Result<()> {
    let labels = shared_network_labels(lifetime);
    anyhow::ensure!(row.name == expected_name && row.labels == labels,
        "network ownership differs from shared generation {}; retaining custody", lifetime.generation());
    Ok(())
}

fn shared_network_labels(
    lifetime: &crate::instance::SharedDockerLifetime,
) -> std::collections::HashMap<String, String> {
    let mut labels = shared_resource_labels(lifetime);
    match lifetime.namespace_owner_kind() {
        crate::instance::SharedDockerOwnerKind::Role => {
            labels.insert("jackin.role".to_owned(), lifetime.namespace_owner().to_owned());
        }
        crate::instance::SharedDockerOwnerKind::Prewarm => {
            labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
            labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
        }
    }
    labels
}

pub(crate) async fn resolve_shared_cleanup_for_state(
    paths: &JackinPaths,
    container_name: &str,
    resources: &DockerResources,
    docker: &impl DockerApi,
) -> anyhow::Result<SharedCleanupPlan> {
    let daemon = docker.daemon_server_id().await?;
    let manifest = read_cleanup_manifest(paths, container_name)?;
    let lifetime = if let Some(lifetime) = crate::instance::SharedDockerLifetime::load_for_cleanup(
        paths, &daemon, container_name,
    )? {
        lifetime
    } else {
        let identity = manifest.docker_identity.as_ref();
        let mut retired = crate::instance::SharedDockerLifetime::retired_for_owner(paths, &daemon, container_name)?
            .into_iter()
            .filter(|lifetime| {
                lifetime.owner_kind() == crate::instance::SharedDockerOwnerKind::Role
                    && lifetime.network_name().unwrap_or_default() == resources.network
                    && lifetime.certs_volume_name() == resources.certs_volume.as_deref()
                    && lifetime.dind_container_name() == resources.dind_container.as_deref()
                    && identity.is_none_or(|identity| {
                        matches!(lifetime.role_container(), crate::instance::SharedContainerCustody::Owned { id, .. } if identity.role_container_id == *id)
                            && identity.dind_container_id.as_deref() == match lifetime.dind_container() {
                                crate::instance::SharedContainerCustody::Owned { id, .. } => Some(id.as_str()),
                                crate::instance::SharedContainerCustody::Pending { .. } | crate::instance::SharedContainerCustody::Disabled => None,
                            }
                            && identity.network_id.as_ref() == lifetime.network_id()
                    })
            })
            .collect::<Vec<_>>();
        anyhow::ensure!(retired.len() == 1,
            "shared Docker lifetime unavailable or ambiguous for {container_name}; retaining custody");
        retired.remove(0)
    };
    anyhow::ensure!(
        resources.role_container == container_name
            && resources.dind_container.as_deref() == match lifetime.dind_container() {
                crate::instance::SharedContainerCustody::Disabled => None,
                crate::instance::SharedContainerCustody::Pending { name }
                | crate::instance::SharedContainerCustody::Owned { name, .. } => Some(name.as_str()),
            }
            && resources.certs_volume.as_deref() == lifetime.certs_volume_name()
            && resources.network == lifetime.network_name().unwrap_or_default(),
        "manifest physical resource names differ from shared lifetime for {container_name}"
    );
    if let Some(identity) = manifest.docker_identity.as_ref() {
        anyhow::ensure!(
            matches!(lifetime.role_container(), crate::instance::SharedContainerCustody::Owned { id, .. } if identity.role_container_id == *id),
            "role ID differs from shared lifetime for {container_name}"
        );
        anyhow::ensure!(
            identity.dind_container_id.as_deref() == match lifetime.dind_container() {
                crate::instance::SharedContainerCustody::Owned { id, .. } => Some(id.as_str()),
                crate::instance::SharedContainerCustody::Pending { .. } | crate::instance::SharedContainerCustody::Disabled => None,
            },
            "DinD ID differs from shared lifetime for {container_name}"
        );
        anyhow::ensure!(identity.network_id.as_ref() == lifetime.network_id(),
            "network ID differs from shared lifetime for {container_name}");
    }
    reconcile_shared_lifetime(paths, lifetime, docker).await
}

pub(crate) async fn ensure_shared_plan_current(
    paths: &JackinPaths,
    plan: &SharedCleanupPlan,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let daemon = docker.daemon_server_id().await?;
    anyhow::ensure!(
        &daemon == plan.lifetime.daemon_server_id(),
        "Docker daemon changed after cleanup admission"
    );
    let durable = if plan.lifetime.is_retired() {
        crate::instance::SharedDockerLifetime::retired_for_owner(paths, &daemon, plan.lifetime.owner())?
            .into_iter()
            .find(|lifetime| lifetime.generation() == plan.lifetime.generation())
    } else {
        crate::instance::SharedDockerLifetime::load_for_cleanup(paths, &daemon, plan.lifetime.owner())?
    };
    anyhow::ensure!(
        durable.as_ref() == Some(&plan.lifetime),
        "shared Docker custody changed after cleanup admission"
    );
    Ok(())
}

async fn ensure_backend_absent_for_purge(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    match super::backend::backend_for_state(paths, container_name) {
        InstanceBackend::Docker => {
            super::backend::DockerBackend::new(docker)
                .ensure_absent_for_purge(paths, container_name)
                .await
        }
        InstanceBackend::AppleContainer => {
            super::backend::AppleContainerBackend::production()
                .ensure_absent_for_purge(paths, container_name)
                .await
        }
    }
}

/// Remove the host-side bind-mount directory used to expose the daemon
/// socket and Capsule launch config into the container. A failure retains the
/// instance custody record so the host-side cleanup can be retried.
pub(crate) async fn admit_socket_removal(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<OwnedRemoval> {
    let paths = paths.clone();
    let dir = paths.jackin_home.join("sockets").join(container_name);
    let displayed = dir.clone();
    let result = jackin_telemetry::spawn::joined_blocking(move || {
        super::coordination::ensure_prunable(&paths, &dir).and_then(|()| {
            OwnedRemoval::admit_contained(&paths.jackin_home, &dir)
        })
    })
    .await;
    let error = match result {
        Ok(Ok(removal)) => return Ok(removal),
        Ok(Err(error)) => error,
        Err(error) => std::io::Error::other(error),
    };
    Err(anyhow::Error::new(error).context(format!(
        "failed to admit socket dir {}; retaining instance custody",
        displayed.display()
    )))
}

pub(crate) async fn remove_admitted_socket(
    socket: OwnedRemoval,
) -> anyhow::Result<()> {
    jackin_telemetry::spawn::joined_blocking(move || socket.remove())
        .await
        .map_err(std::io::Error::other)??;
    Ok(())
}

// ── Orphaned resource garbage collection ─────────────────────────────────

/// Parsed row from `docker ps` for a `DinD` sidecar.
struct DindInfo {
    handle: ContainerHandle,
    role: String,
}

async fn collect_labeled_dind(docker: &impl DockerApi) -> anyhow::Result<Vec<DindInfo>> {
    let rows = docker.list_containers(&[LABEL_KIND_DIND], true).await?;
    let mut sidecars = Vec::new();
    for row in rows {
        if row
            .labels
            .get("jackin.kind")
            .is_some_and(|kind| kind != "dind")
        {
            continue;
        }
        let Some(role) = row.labels.get(LABEL_ROLE_KEY).cloned() else {
            continue;
        };
        if role.is_empty() {
            continue;
        }
        sidecars.push(DindInfo {
            handle: row.handle()?,
            role,
        });
    }
    Ok(sidecars)
}

/// Return `DinD` sidecar containers whose corresponding role container is no
/// longer running.  These are leftovers from hard kills, terminal closures,
/// or startup failures.
fn filter_orphaned_dind(sidecars: Vec<DindInfo>, existing: &[String]) -> Vec<DindInfo> {
    sidecars
        .into_iter()
        .filter(|info| !existing.contains(&info.role))
        .collect()
}

/// Remove orphaned `DinD` containers, their associated role containers, cert
/// volumes, and networks.  Errors are logged but do not abort the launch — GC
/// is best-effort.
pub(super) async fn gc_orphaned_resources(paths: &JackinPaths, docker: &impl DockerApi) {
    let _timing = cleanup_timing("orphaned_resources");
    let sidecars = match collect_labeled_dind(docker).await {
        Ok(v) => v,
        Err(err) => {
            cleanup_failure(format!("GC could not list orphaned DinD containers: {err}"));
            eprintln!(
                "  {} GC: could not list orphaned DinD containers: {err}",
                "warning:".yellow().bold()
            );
            return;
        }
    };

    if sidecars.is_empty() {
        // No orphaned DinD sidecars — still check for orphaned networks.
        gc_orphaned_networks(paths, docker, None).await;
        gc_orphaned_prewarm_dind(paths, docker).await;
        return;
    }

    // Fetch existing roles once; reuse for both orphan detection and network GC.
    let existing_rows = match docker.list_containers(&[LABEL_KIND_ROLE], true).await {
        Ok(v) => v,
        Err(err) => {
            eprintln!(
                "  {} GC: could not list role containers: {err}",
                "warning:".yellow().bold()
            );
            return;
        }
    };
    let existing = existing_rows
        .iter()
        .map(|row| row.name.clone())
        .collect::<Vec<_>>();

    let orphaned = filter_orphaned_dind(sidecars, &existing);

    for info in &orphaned {
        let (resources, role, dind) =
            match resolve_cleanup_handles_for_state(paths, &info.role, None, docker).await {
                Ok(resources) => resources,
                Err(error) => {
                    eprintln!("jackin: GC retained {}: {error}", info.role);
                    continue;
                }
            };
        let Some(dind) = dind else {
            eprintln!(
                "jackin: GC retained {}: recorded sidecar identity is absent",
                info.role
            );
            continue;
        };
        if role.is_some() || dind.id() != info.handle.id() || dind.name() != info.handle.name() {
            eprintln!(
                "jackin: GC retained {}: live role or sidecar identity mismatch",
                info.role
            );
            continue;
        }
        let shared =
            match resolve_shared_cleanup_for_state(paths, &info.role, &resources, docker).await {
                Ok(shared) => shared,
                Err(error) => {
                    eprintln!("jackin: GC retained {}: {error}", info.role);
                    continue;
                }
            };
        if let Err(error) = ensure_shared_plan_current(paths, &shared, docker).await {
            eprintln!("jackin: GC retained {}: {error}", info.role);
            continue;
        }

        // The role is absent by definition of `orphaned`. Remove only the
        // sidecar row's immutable ID; resolving/removing the role by name
        // could destroy a same-name replacement created after the listing.
        let r1 = docker.remove_container_by_id(&dind).await;
        if let Err(err) = &r1 {
            eprintln!(
                "  {} GC of dind sidecar for {}: {err}; refusing shared-resource cleanup",
                "warning:".yellow().bold(),
                info.role
            );
            continue;
        }
        let role_inspection = docker.inspect_container_by_name(&info.role).await;
        if role_inspection.handle.is_some()
            || !matches!(role_inspection.state, ContainerState::NotFound)
        {
            eprintln!(
                "  {} GC of shared resources for {} skipped: role identity is no longer absent",
                "warning:".yellow().bold(),
                info.role
            );
            continue;
        }
        let volume = shared.volume;
        let network = shared.network;
        let (r3, r4) = tokio::join!(
            async {
                if let Some(volume) = volume.as_deref() {
                    docker.remove_volume(volume).await
                } else {
                    Ok(())
                }
            },
            async {
                if let Some(network) = network {
                    docker.remove_network_by_id(&network).await
                } else {
                    Ok(())
                }
            },
        );
        let results = [&r1, &r3, &r4];
        for (result, label) in results
            .iter()
            .zip(["dind sidecar", "certs volume", "network"])
        {
            if let Err(err) = result {
                eprintln!(
                    "  {} GC of {label} for {}: {err}",
                    "warning:".yellow().bold(),
                    info.role
                );
            }
        }
        if results.iter().all(|r| r.is_ok()) {
            eprintln!(
                "        {} orphaned resources for {}",
                "cleaned up".dimmed(),
                info.role
            );
        }
    }

    let existing_set: std::collections::HashSet<String> = existing.into_iter().collect();
    gc_orphaned_networks(paths, docker, Some(&existing_set)).await;
    gc_orphaned_prewarm_dind(paths, docker).await;
}

async fn gc_orphaned_prewarm_dind(paths: &JackinPaths, docker: &impl DockerApi) {
    let state_dind = super::launch::prewarmed_dind_state_container_name(paths);
    let rows = match docker
        .list_containers(&[LABEL_KIND_PREWARM_DIND], true)
        .await
    {
        Ok(rows) => rows,
        Err(err) => {
            eprintln!(
                "  {} GC: could not list orphaned prewarm DinD containers: {err}",
                "warning:".yellow().bold()
            );
            return;
        }
    };
    let Some(state_dind) = state_dind else {
        if !rows.is_empty() {
            eprintln!(
                "  {} GC of prewarm sidecar skipped: retained identity is unavailable",
                "warning:".yellow().bold()
            );
        }
        return;
    };
    for row in rows {
        if state_dind == row.name {
            continue;
        }
        if row.name != "jk-prewarm-dind-dind" {
            continue;
        }
        // No retained generation receipt authorizes this orphan's shared resources.
        eprintln!(
            "jackin: GC retained prewarm sidecar {}: shared resource generation identity unavailable",
            row.name
        );
    }
}

/// Remove jackin-managed Docker networks whose owning role container no longer
/// exists. Pass `Some(existing)` to reuse an already-fetched set of existing
/// role names; pass `None` to fetch fresh (used when no `DinD` sidecars were
/// found and the list was never retrieved).
async fn gc_orphaned_networks(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    existing: Option<&std::collections::HashSet<String>>,
) {
    let _timing = cleanup_timing("orphaned_networks");
    let net_rows = match docker.list_networks(&[LABEL_MANAGED]).await {
        Ok(v) => v,
        Err(err) => {
            cleanup_failure(format!("GC could not list orphaned networks: {err}"));
            eprintln!(
                "  {} GC: could not list orphaned networks: {err}",
                "warning:".yellow().bold()
            );
            return;
        }
    };

    let networks: Vec<(NetworkId, String, String)> = net_rows
        .into_iter()
        .filter_map(|n| {
            let role = n.labels.get(LABEL_ROLE_KEY)?.clone();
            if role.is_empty() {
                return None;
            }
            Some((n.id, n.name, role))
        })
        .collect();

    if networks.is_empty() {
        return;
    }

    let fetched: std::collections::HashSet<String>;
    let existing_set = if let Some(s) = existing {
        std::borrow::Cow::Borrowed(s)
    } else {
        fetched = match list_role_names(docker, true).await {
            Ok(v) => v.into_iter().collect(),
            Err(err) => {
                eprintln!(
                    "  {} GC: could not list role containers: {err}",
                    "warning:".yellow().bold()
                );
                return;
            }
        };
        std::borrow::Cow::Owned(fetched)
    };

    for (network_id, net_name, role) in networks {
        if existing_set.contains(&role) {
            continue;
        }
        let inspection = docker.inspect_container_by_name(&role).await;
        if inspection.handle.is_some() || !matches!(inspection.state, ContainerState::NotFound) {
            eprintln!(
                "  {} GC of network {net_name} skipped: role {role} is no longer absent",
                "warning:".yellow().bold()
            );
            continue;
        }
        let resources = match docker_resources_for_state(paths, &role) {
            Ok(resources) => resources,
            Err(error) => {
                eprintln!("jackin: GC retained network {net_name}: {error}");
                continue;
            }
        };
        let shared = match resolve_shared_cleanup_for_state(paths, &role, &resources, docker).await
        {
            Ok(shared) if shared.network.as_ref() == Some(&network_id) => shared,
            Ok(_) => {
                eprintln!(
                    "jackin: GC retained network {net_name}: generation identity mismatch or absent"
                );
                continue;
            }
            Err(error) => {
                eprintln!("jackin: GC retained network {net_name}: {error}");
                continue;
            }
        };
        if let Err(error) = ensure_shared_plan_current(paths, &shared, docker).await {
            eprintln!("jackin: GC retained network {net_name}: {error}");
            continue;
        }
        if let Err(err) = docker.remove_network_by_id(&network_id).await {
            eprintln!(
                "  {} GC of network {net_name}: {err}",
                "warning:".yellow().bold()
            );
        }
    }
}

pub async fn exile_all(paths: &JackinPaths, docker: &impl DockerApi) -> anyhow::Result<()> {
    let _timing = cleanup_timing("exile_all");
    let _prewarm_writer = super::launch::try_lock_prewarmed_dind(paths)
        .await
        .ok_or_else(|| anyhow::anyhow!("prewarm/adoption writer is active; retry aggregate cleanup"))?;
    let inventory = cleanup_instance_inventory(paths)?;
    admit_aggregate_shared_inventory(paths, &inventory, docker).await?;
    for manifest in &inventory {
        crate::isolation::state::read_records(&paths.data_dir.join(&manifest.container_base))?;
    }
    let mut names = prune_output::start("Finding", "managed containers")
        .complete(list_managed_role_names(docker).await, |error| {
            format!("could not list containers: {error}")
        })?;
    for manifest in &inventory {
        let name = manifest.container_base.clone();
        if !names.iter().any(|existing| existing == &name) {
            names.push(name);
        }
    }
    for lifetime in crate::instance::SharedDockerLifetime::inventory_all(paths)? {
        if !names.iter().any(|existing| existing == lifetime.owner()) {
            names.push(lifetime.owner().to_owned());
        }
    }

    // Admission is complete before the first effect: a later invalid owner or
    // unavailable inspection cannot follow an earlier successful removal.
    let mut plans = Vec::new();
    for name in &names {
        let manifest = inventory
            .iter()
            .find(|manifest| manifest.container_base == *name);
        if let Some(manifest) = manifest
            && matches!(super::backend::backend_for_manifest(Some(manifest)), InstanceBackend::AppleContainer)
        {
            super::backend::AppleContainerBackend::production()
                .ensure_absent_for_purge(paths, name)
                .await?;
            continue;
        }
        let daemon = docker.daemon_server_id().await?;
        let lifetime = crate::instance::SharedDockerLifetime::load_for_cleanup(paths, &daemon, name)?
            .ok_or_else(|| anyhow::anyhow!("shared Docker custody unavailable for {name}; refusing aggregate cleanup"))?;
        let shared = if let Some(manifest) = manifest {
            anyhow::ensure!(matches!(super::backend::backend_for_manifest(Some(manifest)), InstanceBackend::Docker),
                "Docker lifetime conflicts with backend for {name}");
            resolve_shared_cleanup_for_state(paths, name, &manifest.docker, docker).await?
        } else {
            reconcile_shared_lifetime(paths, lifetime, docker).await?
        };
        anyhow::ensure!(shared.lifetime.owner_kind() == crate::instance::SharedDockerOwnerKind::Role
            || manifest.is_none(), "prewarm lifetime unexpectedly has an instance manifest for {name}");
        let socket = if shared.lifetime.owner_kind() == crate::instance::SharedDockerOwnerKind::Role {
            Some(admit_socket_removal(paths, name).await?)
        } else {
            None
        };
        plans.push(DockerCleanupPlan {
            container_base: name.clone(),
            shared,
            socket,
        });
    }

    for plan in &plans {
        ensure_shared_plan_current(paths, &plan.shared, docker).await?;
    }
    let mut retained = Vec::new();
    for plan in plans {
        let name = plan.container_base.clone();
        let result = apply_docker_cleanup_plan(paths, docker, plan).await;
        if let Err(cause) = prune_output::start("Stopping", &name).complete(result, |error| {
            format!("could not remove Docker resources: {error}")
        }) {
            retained.push(RetainedInstanceState {
                container_base: name,
                cause,
            });
        }
    }
    if !retained.is_empty() {
        return Err(InstanceCleanupFailures { retained }.into());
    }
    Ok(())
}

async fn admit_aggregate_shared_inventory(
    paths: &JackinPaths,
    inventory: &[InstanceManifest],
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let lifetimes = crate::instance::SharedDockerLifetime::inventory_all(paths)?;
    if lifetimes.is_empty() {
        return Ok(());
    }
    let daemon = docker.daemon_server_id().await?;
    for lifetime in lifetimes {
        anyhow::ensure!(
            lifetime.daemon_server_id() == &daemon,
            "shared Docker custody belongs to another daemon; retaining generation {}",
            lifetime.generation()
        );
        let manifest = inventory
            .iter()
            .find(|manifest| manifest.container_base == lifetime.owner())
            ;
        if let Some(manifest) = manifest {
            anyhow::ensure!(matches!(super::backend::backend_for_manifest(Some(manifest)), InstanceBackend::Docker),
                "shared Docker custody conflicts with backend for {}; retaining generation {}", lifetime.owner(), lifetime.generation());
            anyhow::ensure!(lifetime.owner_kind() == crate::instance::SharedDockerOwnerKind::Role,
                "prewarm custody conflicts with instance manifest for {}; retaining generation {}", lifetime.owner(), lifetime.generation());
        }
    }
    Ok(())
}

struct DockerCleanupPlan {
    container_base: String,
    shared: SharedCleanupPlan,
    socket: Option<OwnedRemoval>,
}

async fn apply_docker_cleanup_plan(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    mut plan: DockerCleanupPlan,
) -> anyhow::Result<()> {
    ensure_shared_plan_current(paths, &plan.shared, docker).await?;
    plan.shared.lifetime = plan.shared.lifetime.begin_retirement(paths)?;
    ensure_shared_plan_current(paths, &plan.shared, docker).await?;
    if let Some(role) = &plan.shared.role {
        docker.remove_container_by_id(role).await?;
    }
    if let Some(dind) = &plan.shared.dind {
        docker.remove_container_by_id(dind).await?;
    }
    if let Some(volume) = &plan.shared.volume {
        docker.remove_volume(volume).await?;
        anyhow::ensure!(docker.inspect_volume_by_name(volume).await?.is_none(),
            "certificate volume {volume} remains after removal; retaining shared custody");
    }
    if let Some(network) = &plan.shared.network {
        docker.remove_network_by_id(network).await?;
    }
    if let Some(socket) = plan.socket {
        remove_admitted_socket(socket).await?;
    }
    if plan.shared.lifetime.owner_kind() == crate::instance::SharedDockerOwnerKind::Prewarm {
        super::launch::retire_prewarm_projection(paths, &plan.shared.lifetime)?;
    }
    plan.shared.lifetime.retire(paths)?;
    Ok(())
}

// ── Prune ────────────────────────────────────────────────────────────────────

fn prune_dir(
    path: &std::path::Path,
    section_label: &str,
    section_detail: &str,
    target_label: &str,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("prune_dir");
    prune_output::section(section_label, section_detail);
    let row = prune_output::start("Deleting", target_label);
    let result: anyhow::Result<()> = match crate::isolation::safe_remove::safe_remove_dir_all(path)
    {
        Ok(()) => Ok(()),
        Err(error) => Err(anyhow::Error::from(error).context(format!(
            "failed to remove {target_label} at {}",
            path.display()
        ))),
    };
    row.complete(result, |error| {
        cleanup_failure(format!("could not remove {target_label}: {error}"));
        format!("could not remove {target_label}: {error}")
    })
}

pub fn prune_roles(paths: &JackinPaths) -> anyhow::Result<()> {
    super::coordination::ensure_prunable(paths, &paths.roles_dir)?;
    prune_dir(
        &paths.roles_dir,
        "Role Cache",
        "removing cached role repositories",
        "role cache",
    )
}

pub fn prune_cache(paths: &JackinPaths) -> anyhow::Result<()> {
    super::coordination::ensure_prunable(paths, &paths.cache_dir)?;
    prune_dir(
        &paths.cache_dir,
        "Shared Cache",
        "removing rebuildable shared cache",
        "shared cache",
    )
}

pub fn prune_jackin_home(paths: &JackinPaths) -> anyhow::Result<()> {
    super::coordination::ensure_prunable(paths, &paths.jackin_home)?;
    anyhow::ensure!(
        crate::instance::SharedDockerLifetime::inventory_all(paths)?.is_empty(),
        "shared Docker custody remains; refusing to remove runtime home"
    );
    match std::fs::symlink_metadata(&paths.data_dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => anyhow::bail!(
            "instance data remains at {}; refusing to remove runtime home",
            paths.data_dir.display()
        ),
    }
    let _timing = cleanup_timing("runtime_home");
    prune_output::section("Runtime Home", "removing remaining runtime state");
    let row = prune_output::start("Deleting", "runtime home");
    match crate::isolation::safe_remove::safe_remove_dir_all(&paths.jackin_home) {
        Err(err) => {
            cleanup_failure(format!("could not remove runtime home: {err}"));
            row.failed(format!("could not remove runtime home: {err}"));
            return Err(err.into());
        }
        Ok(()) => row.ok(),
    }
    Ok(())
}

/// Remove jk_* Docker images that have no managed role containers (running or stopped).
///
/// Per-image `rmi` failures are printed to stderr and counted in the summary but do not
/// propagate. The initial `docker images` and `docker ps` enumeration calls do propagate.
pub async fn prune_images(docker: &impl DockerApi) -> anyhow::Result<()> {
    let _timing = cleanup_timing("images");
    prune_output::section("Images", "scanning jackin-managed Docker images");
    let all_images = prune_output::start("Finding", "jackin-managed Docker images")
        .complete(docker.list_image_tags("jk_*").await, |error| {
            format!("could not list images: {error}")
        })?;

    if all_images.is_empty() {
        prune_output::ok("no jackin-managed images found");
        return Ok(());
    }

    let role_rows = prune_output::start("Checking", "image usage by role containers").complete(
        docker.list_containers(&[LABEL_KIND_ROLE], true).await,
        |error| format!("could not list role containers: {error}"),
    )?;
    let in_use: std::collections::HashSet<String> = role_rows
        .iter()
        .filter_map(|row| {
            let img_label = row.labels.get(LABEL_IMAGE_KEY).cloned().unwrap_or_default();
            if img_label.is_empty() {
                return None;
            }
            let img = if img_label.contains(':') {
                img_label
            } else {
                format!("{img_label}:latest")
            };
            Some(img)
        })
        .collect();

    let mut removed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for image in &all_images {
        let row = prune_output::start("Deleting", image);
        if in_use.contains(image) {
            row.skip("still used by a role container");
            skipped += 1;
            continue;
        }
        match docker.remove_image(image).await {
            Ok(RemoveImageOutcome::Removed) => {
                row.ok();
                removed += 1;
            }
            Ok(RemoveImageOutcome::InUse) => {
                row.skip("still in use");
                skipped += 1;
            }
            Ok(RemoveImageOutcome::NotFound) => {
                row.skip("already gone");
                skipped += 1;
            }
            Err(error) => {
                cleanup_failure(format!("could not remove image {image}: {error}"));
                row.failed(format!("could not remove: {error}"));
                failed += 1;
            }
        }
    }

    if removed == 0 && failed == 0 {
        if skipped > 0 {
            prune_output::ok(format!("no images removed ({skipped} skipped)"));
        } else {
            prune_output::ok("no unused jackin-managed images to remove");
        }
    } else if failed == 0 {
        prune_output::ok(format!("removed {removed} image(s), skipped {skipped}"));
    } else {
        prune_output::failed(format!(
            "removed {removed} image(s), skipped {skipped}, failed {failed}"
        ));
    }
    Ok(())
}

/// Purge on-disk state for terminated instances and clear their index entries.
///
/// Targets `clean_exited`, `superseded`, `failed_setup`, and `purged`
/// tombstones. Any instance whose filesystem teardown fails — typically because
/// Docker resources are still present — is skipped; use
/// `jackin hardline <selector>` to return or `jackin eject <selector> --purge` to discard.
/// Remove instances with terminal statuses (clean-exited, superseded,
/// failed setup, purged). Does not touch running or restore-available
/// instances. Used by `jackin prune instances`.
pub async fn prune_instances(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("instances");
    prune_output::section("Instances", "scanning terminal instance state");
    // Inventory and every target proof are read-only. A missing index does
    // not authorize publishing a rebuild before later ownership is known.
    let inventory = cleanup_instance_inventory(paths)?;
    if paths.data_dir.join("instances.json").try_exists()? {
        let index = InstanceIndex::read(&paths.data_dir)?;
        for entry in index.instances {
            anyhow::ensure!(
                inventory
                    .iter()
                    .any(|manifest| manifest.container_base == entry.container_base),
                "instance ownership manifest unavailable for {}; retaining index custody",
                entry.container_base
            );
        }
    }
    let mut stale_manifests = Vec::new();
    for manifest in &inventory {
        if manifest.status != InstanceStatus::Active {
            continue;
        }
        crate::isolation::state::read_records(&paths.data_dir.join(&manifest.container_base))?;
        if matches!(
            super::backend::backend_for_manifest(Some(manifest)),
            InstanceBackend::Docker
        ) {
            let (_, role, _) =
                resolve_cleanup_handles_for_state(paths, &manifest.container_base, None, docker)
                    .await?;
            if role.is_none() {
                let mut stale = manifest.clone();
                stale.mark_status(InstanceStatus::Crashed);
                stale_manifests.push(stale);
            }
        }
    }

    let prunable = [
        InstanceStatus::CleanExited,
        InstanceStatus::Superseded,
        InstanceStatus::FailedSetup,
        InstanceStatus::Purged,
    ];

    let mut plans = Vec::new();
    for manifest in &inventory {
        if prunable.contains(&manifest.status) {
            let name = manifest.container_base.clone();
            let plan = admit_container_purge(paths, &name, docker).await?;
            plans.push((name, plan));
        }
    }

    // Stale-active reconciliation starts only after every selected cleanup
    // target has been admitted. Native manifest/index transactions must keep
    // these two durable writes together under the same lifecycle custody.
    for manifest in stale_manifests {
        manifest.write(&paths.data_dir.join(&manifest.container_base))?;
        InstanceIndex::update_manifest(&paths.data_dir, &manifest)?;
    }

    let mut removed: Vec<String> = Vec::new();
    let mut skipped: Vec<(String, anyhow::Error)> = Vec::new();

    for (container_base, plan) in plans {
        let row = prune_output::start("Deleting", &container_base);
        match apply_container_purge(plan, runner).await {
            Ok(()) => {
                row.ok();
                removed.push(container_base);
            }
            Err(error) => {
                row.skip("Docker resources still present");
                skipped.push((container_base, error));
            }
        }
    }

    if !removed.is_empty() {
        let refs: Vec<&str> = removed.iter().map(String::as_str).collect();
        if let Err(error) = InstanceIndex::remove_many(&paths.data_dir, &refs) {
            skipped.push(("instance index".to_owned(), error));
        }
        prune_output::ok(format!("removed state for {} instance(s)", removed.len()));
    } else if skipped.is_empty() {
        prune_output::ok("no instances to prune");
    }

    if !skipped.is_empty() {
        prune_output::failed(format!(
            "cleanup incomplete for {} target(s)",
            skipped.len()
        ));
        for (name, error) in &skipped {
            eprintln!("  {name}: {error}");
        }
        return Err(InstanceCleanupFailures {
            retained: skipped
                .into_iter()
                .map(|(container_base, cause)| RetainedInstanceState {
                    container_base,
                    cause,
                })
                .collect(),
        }
        .into());
    }

    Ok(())
}

/// Force-eject all managed Docker resources then purge every instance's
/// state directory and index entry, regardless of status.
/// Used by `jackin prune instances --all` and `jackin prune system --all`.
pub async fn prune_all_instances(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    super::coordination::ensure_prunable_async(paths, &paths.data_dir).await?;
    // Establish complete filesystem custody before stopping any resources.
    // An index can omit orphaned state; it cannot authorize deleting that state.
    let inventory = cleanup_instance_inventory(paths)?;
    let mut admitted_states = Vec::with_capacity(inventory.len());
    for manifest in &inventory {
        crate::isolation::state::read_records(&paths.data_dir.join(&manifest.container_base))?;
        admitted_states.push((
            manifest.container_base.clone(),
            admit_container_state_removal(paths, &manifest.container_base).await?,
        ));
    }
    prune_output::section(
        "Instances",
        "stopping managed containers and removing all state",
    );
    exile_all(paths, docker).await?;

    jackin_host::caffeinate::reconcile(paths, docker, runner).await;

    if inventory.is_empty() {
        prune_output::ok("no instances to prune");
    } else {
        let containers: Vec<String> = inventory.iter().map(|e| e.container_base.clone()).collect();

        let mut released = Vec::new();
        let mut retained = Vec::new();
        for (container_base, state) in admitted_states {
            let row = prune_output::start("Deleting", &container_base);
            let result =
                match admit_container_purge_with_state(paths, &container_base, docker, state).await
                {
                    Ok(plan) => apply_container_purge(plan, runner).await,
                    Err(error) => Err(error),
                };
            if let Err(err) = result {
                row.failed(format!("isolation cleanup failed: {err}"));
                retained.push(RetainedInstanceState {
                    container_base: container_base.clone(),
                    cause: err,
                });
            } else {
                row.ok();
                released.push(container_base);
            }
        }
        if retained.is_empty() {
            prune_output::ok(format!("pruned {} instance(s)", containers.len()));
        } else {
            prune_output::failed(format!(
                "pruned {} instance(s), cleanup failed for {cleanup_failures}",
                released.len(),
                cleanup_failures = retained.len(),
            ));
            // Record completed siblings while keeping every failed manifest
            // and isolation record recoverable. Never erase their parent.
            let released_names: Vec<&str> = released.iter().map(String::as_str).collect();
            if let Err(error) = InstanceIndex::remove_many(&paths.data_dir, &released_names) {
                retained.push(RetainedInstanceState {
                    container_base: "instance index".to_owned(),
                    cause: error,
                });
            }
            return Err(InstanceCleanupFailures { retained }.into());
        }
    }

    if let Err(err) = crate::isolation::safe_remove::safe_remove_dir_all(&paths.data_dir) {
        prune_output::failed("could not remove instance data");
        return Err(anyhow::Error::from(err).context(format!(
            "failed to remove instance data at {}",
            paths.data_dir.display()
        )));
    }
    Ok(())
}

#[derive(Debug)]
struct RetainedInstanceState {
    container_base: String,
    cause: anyhow::Error,
}

#[derive(Debug)]
struct InstanceCleanupFailures {
    retained: Vec<RetainedInstanceState>,
}

impl std::fmt::Display for InstanceCleanupFailures {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "instance cleanup incomplete; retained recoverable state"
        )?;
        for failure in &self.retained {
            write!(
                formatter,
                "; {}: {:#}",
                failure.container_base, failure.cause
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for InstanceCleanupFailures {}

/// Every state directory requires an exact persisted identity. Missing,
/// malformed, aliased, or symlinked state remains unknown and cannot be purged.
fn cleanup_instance_inventory(paths: &JackinPaths) -> anyhow::Result<Vec<InstanceManifest>> {
    match std::fs::symlink_metadata(&paths.data_dir) {
        Ok(metadata) => anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "instance data is not an owned directory at {}; refusing cleanup",
            paths.data_dir.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    }
    let entries = match std::fs::read_dir(&paths.data_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut manifests = Vec::new();
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        anyhow::ensure!(
            !file_type.is_symlink(),
            "symlinked instance state at {}; refusing cleanup",
            entry.path().display()
        );
        if !file_type.is_dir() {
            anyhow::ensure!(
                file_type.is_file()
                    && matches!(
                        entry.file_name().to_str(),
                        Some(
                            "instances.json"
                                | "instances.json.lock"
                                | "caffeinate.pid"
                                | "caffeinate.lock"
                        )
                    ),
                "unrecognized instance data at {}; refusing cleanup",
                entry.path().display()
            );
            if entry.file_name() == "instances.json" {
                InstanceIndex::read(&paths.data_dir)?;
            }
            continue;
        }
        let file_name = entry.file_name();
        let name = file_name.to_str().ok_or_else(|| {
            anyhow::anyhow!("instance ownership directory has an invalid name; refusing cleanup")
        })?;
        let manifest = read_cleanup_manifest(paths, name)?;
        anyhow::ensure!(
            entry.file_name() == std::ffi::OsStr::new(&manifest.container_base),
            "instance ownership directory mismatch at {}; refusing cleanup",
            entry.path().display()
        );
        RoleSelector::parse(&manifest.role_key).map_err(|error| {
            anyhow::Error::new(error).context(format!(
                "invalid persisted role identity at {}; refusing cleanup",
                entry.path().display()
            ))
        })?;
        manifests.push(manifest);
    }
    manifests.sort_by(|left, right| left.container_base.cmp(&right.container_base));
    Ok(manifests)
}

pub(crate) async fn ensure_role_resources_absent_for_purge(
    docker: &impl DockerApi,
    resources: &DockerResources,
) -> anyhow::Result<()> {
    ensure_container_absent_for_purge(docker, &resources.role_container, "role container").await?;
    if let Some(dind_container) = resources.dind_container.as_deref() {
        ensure_container_absent_for_purge(docker, dind_container, "DinD sidecar").await?;
    }
    Ok(())
}

async fn ensure_container_absent_for_purge(
    docker: &impl DockerApi,
    container_name: &str,
    resource_label: &str,
) -> anyhow::Result<()> {
    let state_phrase = match docker.inspect_container_by_name(container_name).await.state {
        ContainerState::NotFound => return Ok(()),
        ContainerState::Running => "and is running",
        ContainerState::Paused => "and is paused",
        ContainerState::Restarting => "and is restarting",
        ContainerState::Created => "and is being created",
        ContainerState::Removing => "and is being removed",
        ContainerState::Dead => "but is dead",
        ContainerState::Stopped { .. } => "but is stopped",
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!(
                "cannot purge local state for `{container_name}` because Docker resource state could not be inspected: {reason}"
            )
        }
    };
    anyhow::bail!(
        "cannot purge local state because {resource_label} `{container_name}` still exists {state_phrase}; run `jackin eject {container_name} --purge` to remove Docker resources and local state together"
    )
}

#[cfg(test)]
mod tests;
