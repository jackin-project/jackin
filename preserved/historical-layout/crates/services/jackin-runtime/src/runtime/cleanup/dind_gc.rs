// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Orphaned `dind`/network garbage collection.

#![expect(
    clippy::print_stderr,
    reason = "runtime cleanup and GC report operator-visible warnings and results"
)]

use jackin_core::JackinPaths;

use jackin_core::ContainerHandle;
use jackin_docker::docker_client::{ContainerState, DockerApi};
use owo_colors::OwoColorize;

use crate::instance::naming::{dind_certs_volume, role_network_name};
use crate::runtime::discovery::list_role_names;
use crate::runtime::naming::{
    LABEL_KIND_DIND, LABEL_KIND_PREWARM_DIND, LABEL_KIND_ROLE, LABEL_MANAGED, LABEL_ROLE_KEY,
};

use super::{cleanup_failure, cleanup_timing};

/// Parsed row from `docker ps` for a `DinD` sidecar.
pub(crate) struct DindInfo {
    handle: ContainerHandle,
    role: String,
}

pub(crate) async fn collect_labeled_dind(docker: &impl DockerApi) -> anyhow::Result<Vec<DindInfo>> {
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
pub(crate) fn filter_orphaned_dind(sidecars: Vec<DindInfo>, existing: &[String]) -> Vec<DindInfo> {
    sidecars
        .into_iter()
        .filter(|info| !existing.contains(&info.role))
        .collect()
}

/// Remove orphaned `DinD` containers, their associated role containers, cert
/// volumes, and networks.  Errors are logged but do not abort the launch — GC
/// is best-effort.
pub(crate) async fn gc_orphaned_resources(paths: &JackinPaths, docker: &impl DockerApi) {
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
        let certs_volume = dind_certs_volume(&info.role);
        let network = role_network_name(&info.role);

        // The role is absent by definition of `orphaned`. Remove only the
        // sidecar row's immutable ID; resolving/removing the role by name
        // could destroy a same-name replacement created after the listing.
        let r1 = docker.remove_container_by_id(&info.handle).await;
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
        let (r3, r4) = tokio::join!(
            docker.remove_volume(&certs_volume),
            docker.remove_network(&network),
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
    gc_orphaned_networks(docker, Some(&existing_set)).await;
    gc_orphaned_prewarm_dind(paths, docker).await;
}

pub(crate) async fn gc_orphaned_prewarm_dind(paths: &JackinPaths, docker: &impl DockerApi) {
    let state_dind = crate::runtime::launch::prewarmed_dind_state_container_name(paths);
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
        let certs_volume = "jk-prewarm-dind-certs";
        let network = "jk-prewarm-dind-net";
        let Ok(handle) = row.handle() else {
            eprintln!(
                "  {} GC of prewarm sidecar {} skipped: Docker row had no immutable ID",
                "warning:".yellow().bold(),
                row.name
            );
            continue;
        };
        let (r1, r2, r3) = tokio::join!(
            docker.remove_container_by_id(&handle),
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
/// exists. Pass `Some(existing)` to reuse an already-fetched set of existing
/// role names; pass `None` to fetch fresh (used when no `DinD` sidecars were
/// found and the list was never retrieved).
pub(crate) async fn gc_orphaned_networks(
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

    for (net_name, role) in networks {
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
        if let Err(err) = docker.remove_network(&net_name).await {
            eprintln!(
                "  {} GC of network {net_name}: {err}",
                "warning:".yellow().bold()
            );
        }
    }
}
