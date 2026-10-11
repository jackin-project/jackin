// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Role/cache/home/image/instance pruning.
//!
//! Image pruning lives in `jackin_runtime_cleanup_prune_images` (S7 split 99),
//! home pruning in `jackin_runtime_cleanup_prune_home` (S7 split 101),
//! role pruning in `jackin_runtime_cleanup_prune_roles` (S7 split 103),
//! cache pruning in `jackin_runtime_cleanup_prune_cache` (S7 split 104),
//! all re-exported below.

#![expect(
    clippy::print_stderr,
    reason = "runtime cleanup and GC report operator-visible warnings and results"
)]

use crate::instance::{InstanceIndex, InstanceManifest, InstanceStatus};
use crate::runtime::prune_output;
use jackin_core::JackinPaths;

use jackin_core::CommandRunner;
use jackin_docker::docker_client::{ContainerState, DockerApi};
use owo_colors::OwoColorize;

use super::{cleanup_timing, purge_container_filesystem};

// Moved to jackin_runtime_cleanup_prune_images::prune_images (S7 split 99);
// the item re-export keeps every `prune::prune_images` path stable.
pub use jackin_runtime_cleanup_prune_images::prune_images::prune_images;

// Moved to jackin_runtime_cleanup_prune_home::prune_home (S7 split 101);
// the item re-export keeps every `prune::prune_jackin_home` path stable.
pub use jackin_runtime_cleanup_prune_home::prune_home::prune_jackin_home;

// Moved to jackin_runtime_cleanup_prune_roles::prune_roles (S7 split 103);
// the item re-export keeps every `prune::prune_roles` path stable.
pub use jackin_runtime_cleanup_prune_roles::prune_roles::prune_roles;

// Moved to jackin_runtime_cleanup_prune_cache::prune_cache (S7 split 104);
// the item re-export keeps every `prune::prune_cache` path stable.
pub use jackin_runtime_cleanup_prune_cache::prune_cache::prune_cache;

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
