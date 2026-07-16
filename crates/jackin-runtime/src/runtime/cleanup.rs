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

use super::prune_output;
use crate::instance::{DockerResources, InstanceIndex, InstanceManifest, InstanceStatus};
use fs4::FileExt;
use jackin_core::CommandRunner;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::{ContainerState, DockerApi, RemoveImageOutcome};
use owo_colors::OwoColorize;

use super::backend::{ContainerBackend as _, InstanceBackend};
use super::discovery::{list_managed_role_names, list_role_names};
use super::naming::{
    LABEL_IMAGE_KEY, LABEL_KIND_DIND, LABEL_KIND_PREWARM_DIND, LABEL_KIND_ROLE, LABEL_MANAGED,
    LABEL_ROLE_KEY,
};
use crate::instance::naming::{dind_certs_volume, role_network_name};

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

fn cleanup_failure(message: impl AsRef<str>) {
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact("cleanup", message.as_ref());
    }
}

pub async fn purge_class_data(
    paths: &JackinPaths,
    selector: &RoleSelector,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("class_data");
    if !paths.data_dir.exists() {
        return Ok(());
    }

    // Drive each filesystem teardown to completion, then batch the
    // index update for whichever containers succeeded. Returning early
    // on the first failure without recording the prior successes would
    // leave the index claiming the already-deleted state dirs still
    // hold their pre-purge status.
    let role_slug = crate::instance::naming::compact_component(&selector.name, "role");
    let mut matched = Vec::new();
    let mut first_error: Option<anyhow::Error> = None;
    for entry in std::fs::read_dir(&paths.data_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !crate::instance::naming::class_family_matches_with_slug(&role_slug, &file_name) {
            continue;
        }
        match purge_container_filesystem(paths, &file_name, docker, runner).await {
            Ok(()) => matched.push(file_name),
            Err(error) => {
                cleanup_failure(format!("class data purge failed: {error}"));
                first_error = Some(error);
                break;
            }
        }
    }
    let refs: Vec<&str> = matched.iter().map(String::as_str).collect();
    let mark_err = InstanceIndex::mark_many_purged(&paths.data_dir, &refs);
    if let Some(err) = first_error {
        return Err(err);
    }
    mark_err
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
    ensure_backend_absent_for_purge(paths, container_name, docker).await?;
    crate::isolation::cleanup::purge_isolated_for_container(
        &paths.data_dir.join(container_name),
        runner,
    )
    .await?;
    let state_dir = paths.data_dir.join(container_name);
    match std::fs::remove_dir_all(state_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    // Remove the host-side bind-mount dir (~/.jackin/sockets/<container>/)
    // that holds the daemon socket and Capsule launch config. Skipping it
    // here leaks stale `agent.toml` across load/purge cycles; a future
    // launch with the same container basename would bind-mount the old
    // contents before the host's mkdir + write overwrites them.
    remove_socket_dir(paths, container_name);
    // Reap the name-claim lock file (D9). The flock is released when the
    // File handle drops at session end; the file itself must be removed
    // explicitly so it does not accumulate across launch/purge cycles.
    let lock_path = paths.data_dir.join(format!("{container_name}.lock"));
    match std::fs::remove_file(&lock_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            eprintln!(
                "jackin: warning: failed to remove name-claim lock {}: {error}",
                lock_path.display()
            );
        }
    }
    Ok(())
}

pub async fn eject_role(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("eject_role");
    match super::backend::backend_for_state(paths, container_name) {
        InstanceBackend::Docker => {
            super::backend::DockerBackend::new(docker)
                .eject(paths, container_name)
                .await
        }
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
    let resources = docker_resources_for_state(paths, container_name);

    // Remove containers first so the network has no active endpoints.
    docker.remove_container(container_name).await?;
    if let Some(dind_container) = resources.dind_container.as_deref() {
        docker.remove_container(dind_container).await?;
    }

    // Volume and network are independent of each other once containers are gone.
    if let Some(certs_volume) = resources.certs_volume.as_deref() {
        docker.remove_volume(certs_volume).await?;
    }
    docker.remove_network(&resources.network).await?;

    // Best-effort host-side socket dir cleanup. Same reason as
    // purge_container_filesystem above: the daemon socket and the
    // bind-mounted Capsule launch config live under
    // ~/.jackin/sockets/<container>/ and must be removed alongside the
    // docker-side teardown so re-launching the same container basename
    // does not inherit stale state.
    remove_socket_dir(paths, container_name);

    Ok(())
}

pub(crate) fn docker_resources_for_state(
    paths: &JackinPaths,
    container_name: &str,
) -> DockerResources {
    let state_dir = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read_optional(&state_dir).unwrap_or_else(|_| {
        let _warning = jackin_telemetry::record_recovered_degradation();
        None
    });
    manifest.map_or_else(
        || DockerResources::from_container_name(container_name),
        |manifest| manifest.docker,
    )
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
/// socket and Capsule launch config into the container. Best-effort:
/// any failure is logged to stderr but does not abort the surrounding
/// teardown — the docker-side resources are already gone, and a
/// half-removed `~/.jackin/sockets/<container>/` is no worse than the
/// pre-fix steady state.
fn remove_socket_dir(paths: &JackinPaths, container_name: &str) {
    let dir = paths.jackin_home.join("sockets").join(container_name);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!(
            "jackin: warning: failed to remove socket dir {}: {error}",
            dir.display()
        ),
    }
}

// ── Orphaned resource garbage collection ─────────────────────────────────

/// Parsed row from `docker ps` for a `DinD` sidecar.
struct DindInfo {
    name: String,
    role: String,
}

async fn collect_labeled_dind(docker: &impl DockerApi) -> anyhow::Result<Vec<DindInfo>> {
    let rows = docker.list_containers(&[LABEL_KIND_DIND], true).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            if row
                .labels
                .get("jackin.kind")
                .is_some_and(|kind| kind != "dind")
            {
                return None;
            }
            let role = row.labels.get(LABEL_ROLE_KEY)?.clone();
            if role.is_empty() {
                return None;
            }
            Some(DindInfo {
                name: row.name,
                role,
            })
        })
        .collect())
}

/// Return `DinD` sidecar containers whose corresponding role container is no
/// longer running.  These are leftovers from hard kills, terminal closures,
/// or startup failures.
fn filter_orphaned_dind(sidecars: Vec<DindInfo>, running: &[String]) -> Vec<DindInfo> {
    sidecars
        .into_iter()
        .filter(|info| !running.contains(&info.role))
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
        gc_orphaned_networks(docker, None).await;
        gc_orphaned_prewarm_dind(paths, docker).await;
        return;
    }

    // Fetch running roles once; reuse for both orphan detection and network GC.
    let running = match list_role_names(docker, false).await {
        Ok(v) => v,
        Err(err) => {
            eprintln!(
                "  {} GC: could not list running role containers: {err}",
                "warning:".yellow().bold()
            );
            return;
        }
    };

    let orphaned = filter_orphaned_dind(sidecars, &running);

    for info in &orphaned {
        let certs_volume = dind_certs_volume(&info.role);
        let network = role_network_name(&info.role);

        // Remove containers before the network (network rm requires no active endpoints).
        let (r1, r2) = tokio::join!(
            docker.remove_container(&info.role),
            docker.remove_container(&info.name),
        );
        let (r3, r4) = tokio::join!(
            docker.remove_volume(&certs_volume),
            docker.remove_network(&network),
        );
        let results = [&r1, &r2, &r3, &r4];
        for (result, label) in
            results
                .iter()
                .zip(["role container", "dind sidecar", "certs volume", "network"])
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

    gc_orphaned_networks(docker, Some(&running)).await;
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
    for row in rows {
        if state_dind.as_deref() == Some(row.name.as_str()) {
            continue;
        }
        if row.name != "jk-prewarm-dind-dind" {
            continue;
        }
        let certs_volume = "jk-prewarm-dind-certs";
        let network = "jk-prewarm-dind-net";
        let (r1, r2, r3) = tokio::join!(
            docker.remove_container(&row.name),
            docker.remove_volume(certs_volume),
            docker.remove_network(network),
        );
        for (result, label) in [&r1, &r2, &r3].iter().zip([
            "prewarm sidecar",
            "prewarm certs volume",
            "prewarm network",
        ]) {
            if let Err(err) = result {
                eprintln!(
                    "  {} GC of {label} for {}: {err}",
                    "warning:".yellow().bold(),
                    row.name
                );
            }
        }
    }
}

/// Remove jackin-managed Docker networks whose owning role container no longer
/// exists. Pass `Some(running)` to reuse an already-fetched list of running
/// role names; pass `None` to fetch fresh (used when no `DinD` sidecars were
/// found and the list was never retrieved).
async fn gc_orphaned_networks(docker: &impl DockerApi, running: Option<&[String]>) {
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

    let networks: Vec<(String, String)> = net_rows
        .into_iter()
        .filter_map(|n| {
            let role = n.labels.get(LABEL_ROLE_KEY)?.clone();
            if role.is_empty() {
                return None;
            }
            Some((n.name, role))
        })
        .collect();

    if networks.is_empty() {
        return;
    }

    let fetched;
    let running = if let Some(r) = running {
        r
    } else {
        fetched = match list_role_names(docker, false).await {
            Ok(v) => v,
            Err(err) => {
                eprintln!(
                    "  {} GC: could not list running role containers: {err}",
                    "warning:".yellow().bold()
                );
                return;
            }
        };
        &fetched
    };

    for (net_name, role) in networks {
        if running.iter().any(|r| r == &role) {
            continue;
        }
        if let Err(err) = docker.remove_network(&net_name).await {
            eprintln!(
                "  {} GC of network {net_name}: {err}",
                "warning:".yellow().bold()
            );
        }
    }
}

pub async fn exile_all(paths: &JackinPaths, docker: &impl DockerApi) -> anyhow::Result<()> {
    let _timing = cleanup_timing("exile_all");
    let mut names = prune_output::start("Finding", "managed containers")
        .complete(list_managed_role_names(docker).await, |error| {
            format!("could not list containers: {error}")
        })?;
    for name in apple_container_instance_names(paths)? {
        if !names.iter().any(|existing| existing == &name) {
            names.push(name);
        }
    }

    for name in &names {
        prune_output::start("Stopping", name)
            .complete(eject_role(paths, name, docker).await, |error| {
                format!("could not remove Docker resources: {error}")
            })?;
    }
    Ok(())
}

fn apple_container_instance_names(paths: &JackinPaths) -> anyhow::Result<Vec<String>> {
    if !paths.data_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&paths.data_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(manifest) = InstanceManifest::read_optional_lossy(&entry.path()) else {
            continue;
        };
        if matches!(
            super::backend::backend_for_manifest(Some(&manifest)),
            InstanceBackend::AppleContainer
        ) {
            names.push(name);
        }
    }
    Ok(names)
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
    let result: anyhow::Result<()> = match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
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
    prune_dir(
        &paths.roles_dir,
        "Role Cache",
        "removing cached role repositories",
        "role cache",
    )
}

pub fn prune_cache(paths: &JackinPaths) -> anyhow::Result<()> {
    prune_dir(
        &paths.cache_dir,
        "Shared Cache",
        "removing rebuildable shared cache",
        "shared cache",
    )
}

pub fn prune_jackin_home(paths: &JackinPaths) {
    let _timing = cleanup_timing("runtime_home");
    prune_output::section("Runtime Home", "removing remaining runtime state");
    let row = prune_output::start("Deleting", "runtime home");
    match std::fs::remove_dir_all(&paths.jackin_home) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            cleanup_failure(format!("could not remove runtime home: {err}"));
            row.failed(format!("could not remove runtime home: {err}"));
        }
        _ => row.ok(),
    }
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
    let index = prune_output::start("Reading", "instance index")
        .complete(InstanceIndex::read_or_rebuild(&paths.data_dir), |error| {
            format!("could not read instance index: {error}")
        })?;

    // D9: reconcile stale Active rows whose Docker container is gone.
    // A crash mid-session can leave an instance in Active status with no
    // running container. Detect these and transition them to Crashed so they
    // appear as restore candidates on the next launch.
    let stale_active: Vec<String> = index
        .instances
        .iter()
        .filter(|e| e.status == InstanceStatus::Active)
        .map(|e| e.container_base.clone())
        .collect();
    for container_base in stale_active {
        if matches!(
            docker.inspect_container_state(&container_base).await,
            ContainerState::NotFound
        ) {
            let state_dir = paths.data_dir.join(&container_base);
            if let Some(mut manifest) = InstanceManifest::read_optional_lossy(&state_dir) {
                manifest.mark_status(InstanceStatus::Crashed);
                if let Err(err) = manifest.write(&state_dir) {
                    eprintln!(
                        "{} could not update manifest for stale active instance {container_base}: {err}",
                        "warning:".yellow().bold()
                    );
                } else if let Err(err) = InstanceIndex::update_manifest(&paths.data_dir, &manifest)
                {
                    eprintln!(
                        "{} could not update index for stale active instance {container_base}: {err}",
                        "warning:".yellow().bold()
                    );
                }
            }
        }
    }

    let prunable = [
        InstanceStatus::CleanExited,
        InstanceStatus::Superseded,
        InstanceStatus::FailedSetup,
        InstanceStatus::Purged,
    ];

    let candidates: Vec<String> = index
        .instances
        .iter()
        .filter(|e| prunable.contains(&e.status))
        .map(|e| e.container_base.clone())
        .collect();

    let mut removed: Vec<String> = Vec::new();
    let mut skipped: Vec<(String, anyhow::Error)> = Vec::new();

    for container_base in candidates {
        let row = prune_output::start("Deleting", &container_base);
        match purge_container_filesystem(paths, &container_base, docker, runner).await {
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
        let index_updated = match InstanceIndex::remove_many(&paths.data_dir, &refs) {
            Ok(()) => true,
            Err(err) => {
                eprintln!(
                    "{} instance index could not be updated: {err:#}; run `jackin prune instances` again to retry",
                    "warning:".yellow().bold()
                );
                false
            }
        };
        if index_updated {
            prune_output::ok(format!("pruned {} instance(s)", removed.len()));
        } else {
            prune_output::ok(format!(
                "removed state for {} instance(s); index not updated",
                removed.len()
            ));
        }
    } else if skipped.is_empty() {
        prune_output::ok("no instances to prune");
    }

    if !skipped.is_empty() {
        prune_output::skip(format!(
            "skipped {} instance(s); Docker resources still present",
            skipped.len()
        ));
        for (name, error) in &skipped {
            eprintln!("  {name}: {error}");
        }
        eprintln!(
            "Use `jackin eject <selector> --purge` to remove Docker resources and state together."
        );
    }

    // D9: reap orphaned name-claim lock files. Any `<data_dir>/<name>.lock`
    // that is not currently held by a live process (flock acquirable) and has
    // no Active index entry is a leftover that `purge_container_filesystem`
    // should have removed but did not (e.g. the process crashed mid-purge).
    reap_orphaned_name_locks(paths);

    Ok(())
}

/// Remove stale `<data_dir>/<name>.lock` files left behind by crashed or
/// interrupted launches (D9). Tries a non-blocking exclusive lock; if the
/// lock can be acquired the file is unlocked → orphaned → safe to remove.
/// Files held by a live process are left untouched.
fn reap_orphaned_name_locks(paths: &JackinPaths) {
    let Ok(entries) = std::fs::read_dir(&paths.data_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(base) = name.strip_suffix(".lock") else {
            continue;
        };
        // Check whether a live process holds the lock.
        let lock_path = paths.data_dir.join(name.as_ref());
        #[expect(
            clippy::disallowed_methods,
            reason = "lock-holder check requires opening the file to call try_lock"
        )]
        let Ok(file) = std::fs::File::open(&lock_path) else {
            continue;
        };
        if FileExt::try_lock(&file).is_ok() {
            // Lock acquired → no live holder → orphaned.
            drop(file); // Release before removing
            match std::fs::remove_file(&lock_path) {
                Ok(()) => {
                    jackin_diagnostics::telemetry_debug!(
                        "runtime",
                        "reap_orphaned_name_locks: removed orphaned lock for {base}",
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    eprintln!(
                        "jackin: warning: could not remove orphaned lock {}: {error}",
                        lock_path.display()
                    );
                }
            }
        }
    }
}

/// Force-eject all managed Docker resources then purge every instance's
/// state directory and index entry, regardless of status.
/// Used by `jackin prune instances --all` and `jackin prune system --all`.
pub async fn prune_all_instances(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    prune_output::section(
        "Instances",
        "stopping managed containers and removing all state",
    );
    exile_all(paths, docker).await?;

    jackin_host::caffeinate::reconcile(paths, docker, runner).await;

    let index = prune_output::start("Reading", "instance index")
        .complete(InstanceIndex::read_or_rebuild(&paths.data_dir), |error| {
            format!("could not read instance index: {error}")
        })?;
    if index.instances.is_empty() {
        prune_output::ok("no instances to prune");
    } else {
        let containers: Vec<String> = index
            .instances
            .iter()
            .map(|e| e.container_base.clone())
            .collect();

        let mut cleanup_failures = 0usize;
        for container_base in &containers {
            let row = prune_output::start("Deleting", container_base);
            if let Err(err) =
                purge_container_filesystem(paths, container_base, docker, runner).await
            {
                cleanup_failures += 1;
                row.failed(format!("isolation cleanup failed: {err}"));
            } else {
                row.ok();
            }
        }
        if cleanup_failures == 0 {
            prune_output::ok(format!("pruned {} instance(s)", containers.len()));
        } else {
            prune_output::failed(format!(
                "pruned {} instance(s), cleanup failed for {cleanup_failures}",
                containers.len()
            ));
        }
    }

    if let Err(err) = std::fs::remove_dir_all(&paths.data_dir)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        prune_output::failed("could not remove instance data");
        return Err(anyhow::Error::from(err).context(format!(
            "failed to remove instance data at {}",
            paths.data_dir.display()
        )));
    }
    Ok(())
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
    let state_phrase = match docker.inspect_container_state(container_name).await {
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
