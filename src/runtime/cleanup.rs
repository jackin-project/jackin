use crate::docker::CommandRunner;
use crate::docker_client::{DockerApi, RemoveImageOutcome};
use crate::instance::{InstanceIndex, InstanceStatus};
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use owo_colors::OwoColorize;

use super::discovery::{list_managed_role_names, list_role_names};
use super::naming::{
    LABEL_KIND_DIND, LABEL_KIND_ROLE, LABEL_MANAGED, dind_certs_volume, dind_container_name,
    role_network_name,
};

pub async fn purge_class_data(
    paths: &JackinPaths,
    selector: &RoleSelector,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
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
    ensure_role_resources_absent_for_purge(docker, container_name).await?;
    crate::isolation::cleanup::purge_isolated_for_container(
        &paths.data_dir.join(container_name),
        runner,
    ).await?;
    let state_dir = paths.data_dir.join(container_name);
    match std::fs::remove_dir_all(state_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub async fn eject_role(container_name: &str, docker: &impl DockerApi) -> anyhow::Result<()> {
    let dind = dind_container_name(container_name);
    let certs_volume = dind_certs_volume(container_name);
    let network = role_network_name(container_name);

    // Remove containers first so the network has no active endpoints.
    let (r1, r2) = tokio::join!(
        docker.remove_container(container_name),
        docker.remove_container(&dind),
    );
    r1?;
    r2?;

    // Volume and network are independent of each other once containers are gone.
    let (r3, r4) = tokio::join!(
        docker.remove_volume(&certs_volume),
        docker.remove_network(&network),
    );
    r3?;
    r4?;

    Ok(())
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
            let role = row.labels.get("jackin.role")?.to_string();
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
/// Return `DinD` sidecars whose role container is not in `running`.
fn filter_orphaned_dind(sidecars: Vec<DindInfo>, running: &[String]) -> Vec<DindInfo> {
    sidecars
        .into_iter()
        .filter(|info| !running.contains(&info.role))
        .collect()
}

/// Remove orphaned `DinD` containers, their associated role containers, cert
/// volumes, and networks.  Errors are logged but do not abort the launch — GC
/// is best-effort.
pub(super) async fn gc_orphaned_resources(docker: &impl DockerApi) {
    let sidecars = match collect_labeled_dind(docker).await {
        Ok(v) => v,
        Err(err) => {
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
}

/// Remove jackin-managed Docker networks whose owning role container no longer
/// exists. Pass `Some(running)` to reuse an already-fetched list of running
/// role names; pass `None` to fetch fresh (used when no DinD sidecars were
/// found and the list was never retrieved).
async fn gc_orphaned_networks(docker: &impl DockerApi, running: Option<&[String]>) {
    let net_rows = match docker.list_networks(&[LABEL_MANAGED]).await {
        Ok(v) => v,
        Err(err) => {
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
            let role = n.labels.get("jackin.role")?.to_string();
            if role.is_empty() { return None; }
            Some((n.name, role))
        })
        .collect();

    if networks.is_empty() {
        return;
    }

    let fetched;
    let running = match running {
        Some(r) => r,
        None => {
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
        }
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

pub async fn exile_all(docker: &impl DockerApi) -> anyhow::Result<()> {
    let names = list_managed_role_names(docker).await?;
    for name in names {
        eject_role(&name, docker).await?;
    }
    Ok(())
}

// ── Prune ────────────────────────────────────────────────────────────────────

fn prune_dir(path: &std::path::Path, label: &str) -> anyhow::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => println!("Removed {label} ({}).", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("{label} already empty.");
        }
        Err(error) => {
            return Err(anyhow::Error::from(error)
                .context(format!("failed to remove {label} at {}", path.display())));
        }
    }
    Ok(())
}

pub fn prune_roles(paths: &JackinPaths) -> anyhow::Result<()> {
    prune_dir(&paths.roles_dir, "role cache")
}

pub fn prune_cache(paths: &JackinPaths) -> anyhow::Result<()> {
    prune_dir(&paths.cache_dir, "shared cache")
}

pub fn prune_jackin_home(paths: &JackinPaths) {
    match std::fs::remove_dir_all(&paths.jackin_home) {
        Ok(()) => println!("Removed jackin home ({}).", paths.jackin_home.display()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            eprintln!(
                "  {} could not remove {}: {err}",
                "warning:".yellow().bold(),
                paths.jackin_home.display()
            );
        }
    }
}

/// Remove jk_* Docker images that have no jackin-managed role containers (running or stopped).
///
/// Per-image `rmi` failures are printed to stderr and counted in the summary but do not
/// propagate. The initial `docker images` and `docker ps` enumeration calls do propagate.
pub async fn prune_images(docker: &impl DockerApi) -> anyhow::Result<()> {
    let all_images = docker.list_image_tags("jk_*").await?;

    if all_images.is_empty() {
        println!("No jackin-managed images found.");
        return Ok(());
    }

    let role_rows = docker.list_containers(&[LABEL_KIND_ROLE], true).await?;
    let in_use: std::collections::HashSet<String> = role_rows
        .iter()
        .filter_map(|row| {
            let img_label = row.labels.get("jackin.image").cloned().unwrap_or_default();
            if img_label.is_empty() { return None; }
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
        if in_use.contains(image) {
            skipped += 1;
            continue;
        }
        match docker.remove_image(image).await {
            Ok(RemoveImageOutcome::Removed) => removed += 1,
            Ok(RemoveImageOutcome::InUse | RemoveImageOutcome::NotFound) => skipped += 1,
            Err(error) => {
                eprintln!(
                    "  {} could not remove {image}: {error}",
                    "error:".red().bold()
                );
                failed += 1;
            }
        }
    }

    if removed == 0 && failed == 0 {
        if skipped > 0 {
            println!("No images removed ({skipped} skipped).");
        } else {
            println!("No unused jackin-managed images to remove.");
        }
    } else if failed == 0 {
        println!("Removed {removed} image(s), skipped {skipped}.");
    } else {
        eprintln!("Removed {removed} image(s), skipped {skipped}, failed {failed}.");
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
pub async fn prune_instances(paths: &JackinPaths, docker: &impl DockerApi, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir)?;

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

    if candidates.is_empty() {
        println!("No instances to prune.");
        return Ok(());
    }

    let mut removed: Vec<String> = Vec::new();
    let mut skipped: Vec<(String, anyhow::Error)> = Vec::new();

    for container_base in candidates {
        match purge_container_filesystem(paths, &container_base, docker, runner).await {
            Ok(()) => removed.push(container_base),
            Err(error) => skipped.push((container_base, error)),
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
            println!("Pruned {} instance(s):", removed.len());
        } else {
            println!(
                "Removed state for {} instance(s) (index not updated):",
                removed.len()
            );
        }
        for name in &removed {
            println!("  {name}");
        }
    }

    if !skipped.is_empty() {
        eprintln!(
            "Skipped {} instance(s) — Docker resources still present:",
            skipped.len()
        );
        for (name, error) in &skipped {
            eprintln!("  {name}: {error}");
        }
        eprintln!(
            "Use `jackin eject <selector> --purge` to remove Docker resources and state together."
        );
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
    exile_all(docker).await?;

    super::caffeinate::reconcile(paths, docker, runner).await;

    let index = InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    if index.instances.is_empty() {
        println!("No instances to prune.");
    } else {
        let containers: Vec<String> = index
            .instances
            .iter()
            .map(|e| e.container_base.clone())
            .collect();

        println!("Pruned {} instance(s):", containers.len());
        for name in &containers {
            println!("  {name}");
        }
        for container_base in &containers {
            if let Err(err) = purge_container_filesystem(paths, container_base, docker, runner).await {
                eprintln!(
                    "  {} isolation cleanup for {container_base} failed: {err}",
                    "warning:".yellow().bold()
                );
            }
        }
    }

    if let Err(err) = std::fs::remove_dir_all(&paths.data_dir)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        return Err(anyhow::Error::from(err).context(format!(
            "failed to remove instance data at {}",
            paths.data_dir.display()
        )));
    }
    Ok(())
}

async fn ensure_role_resources_absent_for_purge(
    docker: &impl DockerApi,
    container_name: &str,
) -> anyhow::Result<()> {
    ensure_container_absent_for_purge(docker, container_name, "role container").await?;
    ensure_container_absent_for_purge(docker, &format!("{container_name}-dind"), "DinD sidecar").await
}

async fn ensure_container_absent_for_purge(
    docker: &impl DockerApi,
    container_name: &str,
    resource_label: &str,
) -> anyhow::Result<()> {
    let state_phrase = match docker.inspect_container_state(container_name).await {
        crate::docker_client::ContainerState::NotFound => return Ok(()),
        crate::docker_client::ContainerState::Running => "and is running",
        crate::docker_client::ContainerState::Stopped { .. } => "but is stopped",
        crate::docker_client::ContainerState::InspectUnavailable(reason) => {
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
mod tests {
    use super::super::naming::matching_family;
    use super::super::test_support::FakeRunner;
    use super::*;
    use crate::docker_client::{ContainerRow, ContainerState, FakeDockerClient, NetworkRow};
    use crate::paths::JackinPaths;
    use crate::selector::RoleSelector;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    #[tokio::test]
    async fn eject_all_targets_only_requested_class_family() {
        let selector = RoleSelector::new(None, "agent-smith");
        let names = vec![
            "jk-k7p9m2xq-agentsmith".to_string(),
            "jk-a1b2c3d4-myproject-agentsmith".to_string(),
            "jk-w9x8y7z6-chainargos-thearchitect".to_string(),
        ];

        let matched = matching_family(&selector, &names);

        assert_eq!(
            matched,
            vec!["jk-k7p9m2xq-agentsmith", "jk-a1b2c3d4-myproject-agentsmith",]
        );
    }

    #[tokio::test]
    async fn purge_all_removes_matching_state_directories() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let primary = "jk-k7p9m2xq-agentsmith";
        let second = "jk-a1b2c3d4-workspace-agentsmith";
        let manifest =
            crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
                container_base: primary,
                workspace_name: Some("workspace"),
                workspace_label: "workspace",
                workdir: "/workspace",
                host_workdir_fingerprint: "sha256:test",
                role_key: "agent-smith",
                role_display_name: "Agent Smith",
                agent_runtime: crate::agent::Agent::Claude,
                role_source_git: "https://example.invalid/agent-smith.git",
                role_source_ref: None,
                image_tag: "jk_agent-smith",
                docker: crate::instance::DockerResources {
                    role_container: primary.into(),
                    dind_container: format!("{primary}-dind"),
                    network: format!("{primary}-net"),
                    certs_volume: format!("{primary}-dind-certs"),
                },
            });
        manifest.write(&paths.data_dir.join(primary)).unwrap();
        crate::instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
        let second_manifest =
            crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
                container_base: second,
                workspace_name: Some("workspace"),
                workspace_label: "workspace",
                workdir: "/workspace",
                host_workdir_fingerprint: "sha256:test",
                role_key: "agent-smith",
                role_display_name: "Agent Smith",
                agent_runtime: crate::agent::Agent::Claude,
                role_source_git: "https://example.invalid/agent-smith.git",
                role_source_ref: None,
                image_tag: "jk_agent-smith",
                docker: crate::instance::DockerResources {
                    role_container: second.into(),
                    dind_container: format!("{second}-dind"),
                    network: format!("{second}-net"),
                    certs_volume: format!("{second}-dind-certs"),
                },
            });
        second_manifest.write(&paths.data_dir.join(second)).unwrap();
        crate::instance::InstanceIndex::update_manifest(&paths.data_dir, &second_manifest).unwrap();
        let unrelated = "jk-w9x8y7z6-chainargos-thearchitect";
        std::fs::create_dir_all(paths.data_dir.join(unrelated)).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");

        // FakeDockerClient with NotFound for all containers (safe to purge)
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([
            crate::docker_client::ContainerState::NotFound, // primary role container
            crate::docker_client::ContainerState::NotFound, // primary dind
            crate::docker_client::ContainerState::NotFound, // second role container
            crate::docker_client::ContainerState::NotFound, // second dind
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();
        purge_class_data(&paths, &selector, &docker, &mut runner).await.unwrap();

        assert!(!paths.data_dir.join(primary).exists());
        assert!(!paths.data_dir.join(second).exists());
        assert!(paths.data_dir.join(unrelated).exists());
        let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert_eq!(
            index
                .instances
                .iter()
                .filter(|entry| entry.status == crate::instance::InstanceStatus::Purged)
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn purge_container_state_refuses_when_role_container_exists() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-agent-smith";
        std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([
                crate::docker_client::ContainerState::Stopped { exit_code: 0, oom_killed: false },
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = purge_container_state(&paths, container, &docker, &mut runner).await.unwrap_err();

        assert!(
            err.to_string().contains("still exists but is stopped"),
            "got: {err}"
        );
        assert!(err.to_string().contains("jackin eject"), "got: {err}");
        assert!(paths.data_dir.join(container).exists());
    }

    #[tokio::test]
    async fn purge_container_state_refuses_when_dind_sidecar_exists() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-agent-smith";
        std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([
            crate::docker_client::ContainerState::NotFound, // role container not found
            crate::docker_client::ContainerState::Running,  // dind running
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = purge_container_state(&paths, container, &docker, &mut runner).await.unwrap_err();

        assert!(err.to_string().contains("DinD sidecar"), "got: {err}");
        assert!(
            err.to_string().contains("still exists and is running"),
            "got: {err}"
        );
        assert!(paths.data_dir.join(container).exists());
    }

    #[tokio::test]
    async fn eject_agent_removes_container_dind_and_network() {
        let docker = FakeDockerClient::default();

        eject_role("jk-agent-smith", &docker).await.unwrap();

        assert_eq!(
            docker.recorded.borrow().clone(),
            vec![
                "docker rm -f jk-agent-smith",
                "docker rm -f jk-agent-smith-dind",
                "docker volume rm jk-agent-smith-dind-certs",
                "docker network rm jk-agent-smith-net",
            ]
        );
    }

    #[tokio::test]
    async fn eject_agent_ignores_missing_runtime_resources() {
        // FakeDockerClient returns Ok for all operations by default
        // (404 → Ok for remove operations)
        let docker = FakeDockerClient::default();

        eject_role("jk-agent-smith", &docker).await.unwrap();

        assert_eq!(
            docker.recorded.borrow().clone(),
            vec![
                "docker rm -f jk-agent-smith",
                "docker rm -f jk-agent-smith-dind",
                "docker volume rm jk-agent-smith-dind-certs",
                "docker network rm jk-agent-smith-net",
            ]
        );
    }

    #[tokio::test]
    async fn exile_all_ejects_all_managed_agents() {
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
                ContainerRow { name: "jk-k7p9m2xq-agentsmith".to_string(), labels: Default::default() },
                ContainerRow { name: "jk-a1b2c3d4-myworkspace-agentsmith".to_string(), labels: Default::default() },
            ]])),
            ..Default::default()
        };

        exile_all(&docker).await.unwrap();

        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rm -f jk-k7p9m2xq-agentsmith")));
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rm -f jk-a1b2c3d4-myworkspace-agentsmith")));
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker volume rm jk-k7p9m2xq-agentsmith-dind-certs")));
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker network rm jk-k7p9m2xq-agentsmith-net")));
    }

    #[tokio::test]
    async fn exile_all_continues_when_some_runtime_resources_are_missing() {
        // FakeDockerClient treats all remove operations as success (404 is Ok)
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
                ContainerRow { name: "jk-k7p9m2xq-agentsmith".to_string(), labels: Default::default() },
                ContainerRow { name: "jk-a1b2c3d4-myworkspace-agentsmith".to_string(), labels: Default::default() },
            ]])),
            ..Default::default()
        };

        exile_all(&docker).await.unwrap();

        assert_eq!(
            docker.recorded.borrow().clone(),
            vec![
                "docker ps -a --filter jackin.kind=role",
                "docker rm -f jk-k7p9m2xq-agentsmith",
                "docker rm -f jk-k7p9m2xq-agentsmith-dind",
                "docker volume rm jk-k7p9m2xq-agentsmith-dind-certs",
                "docker network rm jk-k7p9m2xq-agentsmith-net",
                "docker rm -f jk-a1b2c3d4-myworkspace-agentsmith",
                "docker rm -f jk-a1b2c3d4-myworkspace-agentsmith-dind",
                "docker volume rm jk-a1b2c3d4-myworkspace-agentsmith-dind-certs",
                "docker network rm jk-a1b2c3d4-myworkspace-agentsmith-net",
            ]
        );
    }

    #[tokio::test]
    async fn gc_removes_orphaned_dind_and_network() {
        let mut labels = std::collections::HashMap::new();
        labels.insert("jackin.role".to_string(), "jk-agent-smith".to_string());
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            // collect_labeled_dind: DinD sidecar with jackin.role label
            vec![ContainerRow { name: "jk-agent-smith-dind".to_string(), labels: labels.clone() }],
            // list_role_names (running): no running role containers
            vec![],
            ])),
            list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await;

        assert!(
            docker.recorded.borrow().iter().any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
        );
        assert!(
            docker.recorded.borrow().iter().any(|c| c.contains("docker rm -f jk-agent-smith"))
        );
        assert!(
            docker.recorded.borrow().iter().any(|c| c.contains("docker volume rm jk-agent-smith-dind-certs"))
        );
        assert!(
            docker.recorded.borrow().iter().any(|c| c.contains("docker network rm jk-agent-smith-net"))
        );
    }

    #[tokio::test]
    async fn gc_skips_dind_when_agent_is_running() {
        let mut labels = std::collections::HashMap::new();
        labels.insert("jackin.role".to_string(), "jk-agent-smith".to_string());
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            // collect_labeled_dind: DinD sidecar present
            vec![ContainerRow { name: "jk-agent-smith-dind".to_string(), labels: labels.clone() }],
            // list_role_names (running): role IS running — skip GC
            vec![ContainerRow { name: "jk-agent-smith".to_string(), labels: Default::default() }],
            ])),
            list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await;

        assert!(
            !docker.recorded.borrow().iter().any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
        );
    }

    #[tokio::test]
    async fn gc_does_nothing_when_no_orphans() {
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // collect_labeled_dind: no DinD
            list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),   // gc_orphaned_networks: no networks
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await;

        assert!(!docker.recorded.borrow().iter().any(|c| c.contains("docker rm")));
    }

    #[tokio::test]
    async fn gc_removes_orphaned_network_without_dind() {
        let mut net_labels = std::collections::HashMap::new();
        net_labels.insert("jackin.role".to_string(), "jk-agent-smith".to_string());
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![], // collect_labeled_dind: no DinD sidecars
            // list_role_names (running) for gc_orphaned_networks: role not running
            vec![],
            ])),
            list_networks_queue: std::cell::RefCell::new(VecDeque::from([
            // gc_orphaned_networks: has a network with jackin.role label
            vec![NetworkRow {
            name: "jk-agent-smith-net".to_string(),
            labels: net_labels,
            }],
            ])),
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await;

        assert!(
            docker.recorded.borrow().iter().any(|c| c.contains("docker network rm jk-agent-smith-net"))
        );
    }

    #[tokio::test]
    async fn gc_cleans_multiple_orphans() {
        let mut labels_smith = std::collections::HashMap::new();
        labels_smith.insert("jackin.role".to_string(), "jk-agent-smith".to_string());
        let mut labels_neo = std::collections::HashMap::new();
        labels_neo.insert("jackin.role".to_string(), "jk-neo".to_string());
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            // collect_labeled_dind: two orphaned DinD sidecars
            vec![
            ContainerRow { name: "jk-agent-smith-dind".to_string(), labels: labels_smith },
            ContainerRow { name: "jk-neo-dind".to_string(), labels: labels_neo },
            ],
            // list_role_names (running): no running roles
            vec![],
            ])),
            list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await;

        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rm -f jk-agent-smith-dind")));
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker volume rm jk-agent-smith-dind-certs")));
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rm -f jk-neo-dind")));
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker volume rm jk-neo-dind-certs")));
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker network rm jk-neo-net")));
    }

    #[tokio::test]
    async fn gc_does_not_panic_when_collect_orphaned_dind_fails() {
        // Docker daemon unreachable — the DinD ps call fails. gc_orphaned_resources
        // must emit a warning and return without panicking.
        let docker = FakeDockerClient {
            fail_with: vec![(
                "jackin.kind=dind".to_string(),
                "Error response from daemon: socket timeout".to_string(),
            )],
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await; // must not panic
    }

    #[tokio::test]
    async fn gc_does_not_panic_when_network_ls_fails() {
        // DinD list succeeds (no orphans), but docker network ls fails.
        // gc_orphaned_networks must emit a warning and return without panicking.
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // no DinD sidecars
            fail_with: vec![(
                "docker network ls".to_string(),
                "Error response from daemon: socket timeout".to_string(),
            )],
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await; // must not panic
    }

    #[tokio::test]
    async fn gc_does_not_panic_when_list_role_names_fails_in_orphaned_networks() {
        // Network ls succeeds (non-empty), but the docker ps to list running roles fails.
        // gc_orphaned_networks must emit a warning and return without calling network rm.
        let mut net_labels = std::collections::HashMap::new();
        net_labels.insert("jackin.role".to_string(), "jk-agent-smith".to_string());
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![], // collect_labeled_dind: no DinD
            // list_role_names call inside gc_orphaned_networks will fail via fail_with
            ])),
            list_networks_queue: std::cell::RefCell::new(VecDeque::from([
            vec![NetworkRow {
            name: "jk-agent-smith-net".to_string(),
            labels: net_labels,
            }],
            ])),
            fail_with: vec![(
                "jackin.kind=role".to_string(),
                "Error response from daemon: socket timeout".to_string(),
            )],
            ..Default::default()
        };

        gc_orphaned_resources(&docker).await; // must not panic

        assert!(
            !docker.recorded.borrow().iter().any(|c| c.contains("docker network rm"))
        );
    }

    // ── prune_dir ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn prune_dir_removes_existing_directory() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cache");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("file.txt"), b"data").unwrap();

        prune_dir(&target, "cache").unwrap();

        assert!(!target.exists());
    }

    #[tokio::test]
    async fn prune_dir_is_ok_when_directory_absent() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cache");

        prune_dir(&target, "cache").unwrap();
    }

    // ── prune_instances ──────────────────────────────────────────────────────

    fn make_instance_at(
        paths: &JackinPaths,
        container: &str,
        status: crate::instance::InstanceStatus,
    ) {
        let mut manifest =
            crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
                container_base: container,
                workspace_name: Some("ws"),
                workspace_label: "ws",
                workdir: "/ws",
                host_workdir_fingerprint: "sha256:test",
                role_key: "agent-smith",
                role_display_name: "Agent Smith",
                agent_runtime: crate::agent::Agent::Claude,
                role_source_git: "https://example.invalid/agent-smith.git",
                role_source_ref: None,
                image_tag: "jk_agent-smith",
                docker: crate::instance::DockerResources {
                    role_container: container.to_string(),
                    dind_container: format!("{container}-dind"),
                    network: format!("{container}-net"),
                    certs_volume: format!("{container}-dind-certs"),
                },
            });
        manifest.mark_status(status);
        let state_dir = paths.data_dir.join(container);
        std::fs::create_dir_all(&state_dir).unwrap();
        manifest.write(&state_dir).unwrap();
        crate::instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    }

    #[tokio::test]
    async fn prune_instances_removes_terminal_statuses_only() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let prunable = "jk-k7p9m2xq-agentsmith";
        let kept = "jk-a1b2c3d4-agentsmith";
        make_instance_at(
            &paths,
            prunable,
            crate::instance::InstanceStatus::CleanExited,
        );
        make_instance_at(&paths, kept, crate::instance::InstanceStatus::Crashed);

        let docker = FakeDockerClient::default(); // inspect returns NotFound → allow purge
        let mut runner = FakeRunner::default();
        prune_instances(&paths, &docker, &mut runner).await.unwrap();

        assert!(!paths.data_dir.join(prunable).exists());
        assert!(paths.data_dir.join(kept).exists());
        let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert!(index.instances.iter().all(|e| e.container_base != prunable));
        assert!(index.instances.iter().any(|e| e.container_base == kept));
    }

    #[tokio::test]
    async fn prune_instances_skips_when_docker_resources_present() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-k7p9m2xq-agentsmith";
        make_instance_at(
            &paths,
            container,
            crate::instance::InstanceStatus::CleanExited,
        );

        // inspect_queue returns Running → container still exists → skip purge.
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();
        prune_instances(&paths, &docker, &mut runner).await.unwrap();

        assert!(paths.data_dir.join(container).exists());
        let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert!(
            index
                .instances
                .iter()
                .any(|e| e.container_base == container)
        );
    }

    #[tokio::test]
    async fn prune_instances_is_ok_when_data_dir_absent() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        let docker = FakeDockerClient::default();
        let mut runner = FakeRunner::default();
        prune_instances(&paths, &docker, &mut runner).await.unwrap();
    }

    // ── prune_images ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn prune_images_skips_images_in_use_by_role_containers() {
        // Image listed, but a role container has jackin.image label pointing to it.
        let mut image_labels = std::collections::HashMap::new();
        image_labels.insert("jackin.image".to_string(), "jk_agent-smith".to_string());
        let docker = FakeDockerClient {
            list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec!["jk_agent-smith:latest".to_string()]])),
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
                ContainerRow { name: "jk-foo".to_string(), labels: image_labels },
            ]])),
            ..Default::default()
        };

        prune_images(&docker).await.unwrap();

        assert!(!docker.recorded.borrow().iter().any(|c| c.contains("docker rmi")));
    }

    #[tokio::test]
    async fn prune_images_counts_rmi_in_use_error_as_skipped_not_failed() {
        // Image passes the pre-filter (not in the in_use set from list_containers)
        // but remove_image returns InUse. prune_images must still return Ok.
        let docker = FakeDockerClient {
            list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec!["jk_agent-smith:latest".to_string()]])),
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // no containers in index
            remove_image_queue: std::cell::RefCell::new(VecDeque::from([crate::docker_client::RemoveImageOutcome::InUse])),
            ..Default::default()
        };

        prune_images(&docker).await.unwrap();

        // rmi was attempted (image was not in the pre-filter set)
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rmi jk_agent-smith:latest")));
    }

    #[tokio::test]
    async fn prune_images_removes_images_not_in_use() {
        let docker = FakeDockerClient {
            list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec!["jk_agent-smith:latest".to_string()]])),
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // no containers
            ..Default::default()
        };

        prune_images(&docker).await.unwrap();

        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rmi jk_agent-smith:latest")));
    }

    #[tokio::test]
    async fn prune_images_is_ok_when_no_images_found() {
        let docker = FakeDockerClient {
            list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
            ..Default::default()
        };

        prune_images(&docker).await.unwrap();

        assert!(!docker.recorded.borrow().iter().any(|c| c.contains("docker rmi")));
    }

    #[tokio::test]
    async fn prune_images_is_ok_when_rmi_fails_with_real_error() {
        // A real Docker error (not in-use, not missing) is printed to stderr
        // but prune_images still returns Ok — best-effort cleanup.
        let docker = FakeDockerClient {
            list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec!["jk_agent-smith:latest".to_string()]])),
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
            fail_with: vec![(
                "docker rmi jk_agent-smith:latest".to_string(),
                "Error response from daemon: permission denied".to_string(),
            )],
            ..Default::default()
        };

        prune_images(&docker).await.unwrap();

        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rmi jk_agent-smith:latest")));
    }

    #[tokio::test]
    async fn prune_images_mixed_removed_and_skipped() {
        // One image is in-use (pre-filtered via jackin.image label), one is removed.
        let mut image_labels = std::collections::HashMap::new();
        image_labels.insert("jackin.image".to_string(), "jk_neo".to_string()); // no :tag → jk_neo:latest
        let docker = FakeDockerClient {
            list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![
                "jk_agent-smith:latest".to_string(),
                "jk_neo:latest".to_string(),
            ]])),
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
                ContainerRow { name: "jk-bar".to_string(), labels: image_labels },
            ]])),
            ..Default::default()
        };

        prune_images(&docker).await.unwrap();

        // Only jk_agent-smith:latest should have had rmi attempted.
        assert!(docker.recorded.borrow().iter().any(|c| c.contains("docker rmi jk_agent-smith:latest")));
        assert!(!docker.recorded.borrow().iter().any(|c| c.contains("docker rmi jk_neo:latest")));
    }

    #[tokio::test]
    async fn prune_images_skips_when_image_disappears_between_list_and_rmi() {
        // TOCTOU: image listed but already gone by rmi time — should be skipped, not failed.
        let docker = FakeDockerClient {
            list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec!["jk_agent-smith:latest".to_string()]])),
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
            remove_image_queue: std::cell::RefCell::new(VecDeque::from([crate::docker_client::RemoveImageOutcome::NotFound])),
            ..Default::default()
        };

        prune_images(&docker).await.unwrap();
    }

    #[tokio::test]
    async fn prune_instances_removes_all_four_prunable_statuses() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let clean = "jk-a1b2c3d4-agentsmith";
        let superseded = "jk-b2c3d4e5-agentsmith";
        let failed = "jk-c3d4e5f6-agentsmith";
        let purged = "jk-d4e5f6a7-agentsmith";
        let crashed = "jk-e5f6a7b8-agentsmith";
        make_instance_at(&paths, clean, crate::instance::InstanceStatus::CleanExited);
        make_instance_at(
            &paths,
            superseded,
            crate::instance::InstanceStatus::Superseded,
        );
        make_instance_at(&paths, failed, crate::instance::InstanceStatus::FailedSetup);
        make_instance_at(&paths, purged, crate::instance::InstanceStatus::Purged);
        make_instance_at(&paths, crashed, crate::instance::InstanceStatus::Crashed);

        let docker = FakeDockerClient::default(); // inspect returns NotFound → allow purge
        let mut runner = FakeRunner::default();
        prune_instances(&paths, &docker, &mut runner).await.unwrap();

        let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        for name in [clean, superseded, failed, purged] {
            assert!(
                !paths.data_dir.join(name).exists(),
                "{name} should be pruned"
            );
            assert!(
                index.instances.iter().all(|e| e.container_base != name),
                "{name} should be absent from index"
            );
        }
        assert!(
            paths.data_dir.join(crashed).exists(),
            "crashed should be kept"
        );
    }

    #[tokio::test]
    async fn prune_instances_prunes_purged_tombstone_with_no_state_directory() {
        // Purged tombstones are index-only entries — the state dir is already gone.
        // purge_container_filesystem must tolerate NotFound so the tombstone is
        // removed from the index without error.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-k7p9m2xq-agentsmith";
        // Register in the index but do NOT create the state directory.
        let manifest =
            crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
                container_base: container,
                workspace_name: Some("ws"),
                workspace_label: "ws",
                workdir: "/ws",
                host_workdir_fingerprint: "sha256:test",
                role_key: "agent-smith",
                role_display_name: "Agent Smith",
                agent_runtime: crate::agent::Agent::Claude,
                role_source_git: "https://example.invalid/agent-smith.git",
                role_source_ref: None,
                image_tag: "jk_agent-smith",
                docker: crate::instance::DockerResources {
                    role_container: container.to_string(),
                    dind_container: format!("{container}-dind"),
                    network: format!("{container}-net"),
                    certs_volume: format!("{container}-dind-certs"),
                },
            });
        let mut manifest = manifest;
        manifest.mark_status(crate::instance::InstanceStatus::Purged);
        crate::instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

        let docker = FakeDockerClient::default(); // inspect returns NotFound → allow purge
        let mut runner = FakeRunner::default();
        prune_instances(&paths, &docker, &mut runner).await.unwrap();

        let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert!(
            index
                .instances
                .iter()
                .all(|e| e.container_base != container),
            "tombstone should be cleared from the index"
        );
    }

    #[tokio::test]
    async fn prune_dir_returns_err_with_path_context_on_failure() {
        // Create a file at the path so remove_dir_all fails (ENOTDIR on the
        // path's parent, or similar — exact error is platform-dependent but
        // it will not be NotFound).
        let temp = tempdir().unwrap();
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        let target = blocker.join("child"); // child of a file — cannot exist

        let err = prune_dir(&target, "test label").unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("failed to remove test label"), "got: {msg}");
        assert!(msg.contains("child"), "got: {msg}");
    }

    #[tokio::test]
    async fn prune_all_instances_removes_data_dir_entirely() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::write(paths.data_dir.join("jk-abc123-thearchitect.lock"), b"").unwrap();
        std::fs::write(paths.data_dir.join("caffeinate.lock"), b"").unwrap();
        std::fs::write(paths.data_dir.join("caffeinate.pid"), b"99999").unwrap();
        std::fs::create_dir_all(paths.data_dir.join("the-architect.locks")).unwrap();
        std::fs::write(
            paths
                .data_dir
                .join("the-architect.locks")
                .join("default.repo.lock"),
            b"",
        )
        .unwrap();

        let docker = FakeDockerClient::default(); // exile_all: list_containers returns empty
        let mut runner = FakeRunner::default();
        prune_all_instances(&paths, &docker, &mut runner).await.unwrap();

        assert!(
            !paths.data_dir.exists(),
            "data_dir should be completely removed"
        );
    }

    #[tokio::test]
    async fn prune_all_instances_removes_data_dir_when_index_empty() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::write(paths.data_dir.join("jk-stale.lock"), b"").unwrap();

        let docker = FakeDockerClient::default();
        let mut runner = FakeRunner::default();
        prune_all_instances(&paths, &docker, &mut runner).await.unwrap();

        assert!(!paths.data_dir.exists(), "data_dir removed");
    }

    // ── prune_jackin_home ────────────────────────────────────────────────────

    #[tokio::test]
    async fn prune_jackin_home_removes_home() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(paths.jackin_home.join("leftover")).unwrap();

        prune_jackin_home(&paths);

        assert!(!paths.jackin_home.exists(), "jackin_home should be removed");
    }

    #[tokio::test]
    async fn prune_jackin_home_is_ok_when_absent() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        // jackin_home never created — must not panic
        prune_jackin_home(&paths);
    }
}
