// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Unused `jk_*` Docker image pruning.
//!
//! [`prune_images`] removes jackin-managed images that no role container
//! (running or stopped) still references, reporting per-image progress
//! through the shared prune-output rows. Best-effort per image: `rmi`
//! failures print and count into the summary without propagating, while
//! the initial image/container enumeration errors do propagate.
//! Instance and role pruning stays in the `jackin-runtime` hub.

use jackin_docker::docker_client::{DockerApi, RemoveImageOutcome};
use jackin_runtime_cleanup_timing::timing::{cleanup_failure, cleanup_timing};
use jackin_runtime_naming::naming::{LABEL_IMAGE_KEY, LABEL_KIND_ROLE};
use jackin_runtime_prune_output::prune_output;

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
