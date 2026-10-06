// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Role/cache/home/image/instance pruning.

#![expect(
    clippy::print_stderr,
    reason = "runtime cleanup and GC report operator-visible warnings and results"
)]

use crate::instance::{InstanceIndex, InstanceManifest, InstanceStatus};
use crate::runtime::prune_output;
use jackin_core::JackinPaths;

use jackin_core::CommandRunner;
use jackin_docker::docker_client::{ContainerState, DockerApi, RemoveImageOutcome};
use owo_colors::OwoColorize;

use crate::runtime::naming::{LABEL_IMAGE_KEY, LABEL_KIND_ROLE};

use super::{cleanup_failure, cleanup_timing, prune_dir, purge_container_filesystem};

pub fn prune_roles(paths: &JackinPaths) -> anyhow::Result<()> {
    crate::runtime::coordination::ensure_prunable(paths, &paths.roles_dir)?;
    prune_dir(
        &paths.roles_dir,
        "Role Cache",
        "removing cached role repositories",
        "role cache",
    )
}

pub fn prune_cache(paths: &JackinPaths) -> anyhow::Result<()> {
    crate::runtime::coordination::ensure_prunable(paths, &paths.cache_dir)?;
    prune_dir(
        &paths.cache_dir,
        "Shared Cache",
        "removing rebuildable shared cache",
        "shared cache",
    )
}

pub fn prune_jackin_home(paths: &JackinPaths) -> anyhow::Result<()> {
    crate::runtime::coordination::ensure_prunable(paths, &paths.jackin_home)?;
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
            docker
                .inspect_container_by_name(&container_base)
                .await
                .state,
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

    Ok(())
}
