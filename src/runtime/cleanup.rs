use crate::docker::CommandRunner;
use crate::instance::{InstanceIndex, InstanceStatus};
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use owo_colors::OwoColorize;

use super::discovery::{list_managed_role_names, list_role_names};
use super::naming::{FILTER_KIND_DIND, FILTER_KIND_ROLE, FILTER_MANAGED, dind_certs_volume};

pub fn purge_class_data(
    paths: &JackinPaths,
    selector: &RoleSelector,
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
        match purge_container_filesystem(paths, &file_name, runner) {
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

pub fn purge_container_state(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    purge_container_filesystem(paths, container_name, runner)?;
    InstanceIndex::mark_purged(&paths.data_dir, container_name)
}

/// Per-container filesystem teardown (docker-state guard + isolation
/// cleanup + state directory removal). Index updates are batched by the
/// caller so multi-container purges avoid an O(M²) read-rewrite cycle.
fn purge_container_filesystem(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    ensure_role_resources_absent_for_purge(runner, container_name)?;
    crate::isolation::cleanup::purge_isolated_for_container(
        &paths.data_dir.join(container_name),
        runner,
    )?;
    let state_dir = paths.data_dir.join(container_name);
    match std::fs::remove_dir_all(state_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn eject_role(container_name: &str, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let dind = format!("{container_name}-dind");
    let certs_volume = dind_certs_volume(container_name);
    let network = format!("{container_name}-net");

    run_cleanup_command(runner, &["rm", "-f", container_name])?;
    run_cleanup_command(runner, &["rm", "-f", &dind])?;
    run_cleanup_command(runner, &["volume", "rm", &certs_volume])?;
    run_cleanup_command(runner, &["network", "rm", &network])?;

    Ok(())
}

pub(super) fn run_cleanup_command(
    runner: &mut impl CommandRunner,
    args: &[&str],
) -> anyhow::Result<()> {
    match runner.capture("docker", args, None) {
        Ok(_) => Ok(()),
        Err(error) if is_missing_cleanup_error(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn is_missing_cleanup_error(error: &anyhow::Error) -> bool {
    crate::docker::is_missing_resource_error(&error.to_string())
}

// ── Orphaned resource garbage collection ─────────────────────────────────

/// Parsed row from `docker ps` for a `DinD` sidecar.
struct DindInfo {
    name: String,
    role: String,
}

fn collect_labeled_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let dind_output = runner.capture(
        "docker",
        &[
            "ps",
            "-a",
            "--filter",
            FILTER_KIND_DIND,
            "--format",
            "{{.Names}}\t{{.Label \"jackin.role\"}}",
        ],
        None,
    )?;

    Ok(dind_output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (name, role) = line.split_once('\t')?;
            if role.is_empty() {
                return None;
            }
            Some(DindInfo {
                name: name.to_string(),
                role: role.to_string(),
            })
        })
        .collect())
}

/// Return `DinD` sidecar containers whose corresponding role container is no
/// longer running.  These are leftovers from hard kills, terminal closures,
/// or startup failures.
fn collect_orphaned_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let sidecars = collect_labeled_dind(runner)?;

    if sidecars.is_empty() {
        return Ok(vec![]);
    }

    // Running role containers (label filter excludes DinD sidecars).
    let running = list_role_names(runner, false)?;

    Ok(sidecars
        .into_iter()
        .filter(|info| !running.contains(&info.role))
        .collect())
}

/// Remove orphaned `DinD` containers, their associated role containers, cert
/// volumes, and networks.  Errors are logged but do not abort the launch — GC
/// is best-effort.
pub(super) fn gc_orphaned_resources(runner: &mut impl CommandRunner) {
    let Ok(orphaned) = collect_orphaned_dind(runner) else {
        return;
    };

    for info in &orphaned {
        let certs_volume = dind_certs_volume(&info.role);
        let network = format!("{}-net", info.role);

        let results = [
            run_cleanup_command(runner, &["rm", "-f", &info.role]),
            run_cleanup_command(runner, &["rm", "-f", &info.name]),
            run_cleanup_command(runner, &["volume", "rm", &certs_volume]),
            run_cleanup_command(runner, &["network", "rm", &network]),
        ];
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
        if results.iter().any(Result::is_ok) {
            eprintln!(
                "        {} orphaned resources for {}",
                "cleaned up".dimmed(),
                info.role
            );
        }
    }

    // Clean up any orphaned networks that survived without a DinD container
    // (e.g. the DinD container was manually removed but the network lingers).
    gc_orphaned_networks(runner);
}

/// Remove jackin-managed Docker networks whose owning role container no
/// longer exists.
fn gc_orphaned_networks(runner: &mut impl CommandRunner) {
    let Ok(net_output) = runner.capture(
        "docker",
        &[
            "network",
            "ls",
            "--filter",
            FILTER_MANAGED,
            "--format",
            "{{.Name}}\t{{.Label \"jackin.role\"}}",
        ],
        None,
    ) else {
        return;
    };

    let networks: Vec<(&str, &str)> = net_output
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, role)| !role.is_empty())
        .collect();

    if networks.is_empty() {
        return;
    }

    let Ok(running) = list_role_names(runner, false) else {
        return;
    };

    for (net_name, role) in networks {
        if running.iter().any(|r| r == role) {
            continue;
        }
        if let Err(err) = run_cleanup_command(runner, &["network", "rm", net_name]) {
            eprintln!(
                "  {} GC of network {net_name}: {err}",
                "warning:".yellow().bold()
            );
        }
    }
}

pub fn exile_all(runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let names = list_managed_role_names(runner)?;
    for name in names {
        eject_role(&name, runner)?;
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

/// Re-cloned on next launch.
pub fn prune_roles(paths: &JackinPaths) -> anyhow::Result<()> {
    prune_dir(&paths.roles_dir, "role cache")
}

/// Terminfo and version-check caches regenerate on first use.
pub fn prune_cache(paths: &JackinPaths) -> anyhow::Result<()> {
    prune_dir(&paths.cache_dir, "shared cache")
}

/// Remove jk-* Docker images that have no jackin-managed role containers (running or stopped).
///
/// Per-image `rmi` failures are printed to stderr and counted in the summary but do not
/// propagate. The initial `docker images` and `docker ps` enumeration calls do propagate.
pub fn prune_images(runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let images_output = runner.capture(
        "docker",
        &[
            "images",
            "--filter",
            "reference=jk-*",
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ],
        None,
    )?;

    let all_images: Vec<String> = images_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    if all_images.is_empty() {
        println!("No jackin-managed images found.");
        return Ok(());
    }

    let in_use_output = runner.capture(
        "docker",
        &[
            "ps",
            "-a",
            "--filter",
            FILTER_KIND_ROLE,
            "--format",
            "{{.Image}}",
        ],
        None,
    )?;

    let in_use: std::collections::HashSet<String> = in_use_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|img| {
            if img.contains(':') {
                img.to_string()
            } else {
                format!("{img}:latest")
            }
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
        match runner.capture("docker", &["rmi", image], None) {
            Ok(_) => removed += 1,
            Err(error) => {
                let msg = error.to_string();
                if crate::docker::is_image_in_use_error(&msg)
                    || crate::docker::is_missing_resource_error(&msg)
                {
                    skipped += 1;
                } else {
                    eprintln!("  could not remove {image}: {error}");
                    failed += 1;
                }
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
        println!("Removed {removed} image(s), skipped {skipped}, failed {failed}.");
    }
    Ok(())
}

/// Purge on-disk state for terminated instances and clear their index entries.
///
/// Targets `clean_exited`, `superseded`, `failed_setup`, and `purged`
/// tombstones. Any instance whose Docker resources are still present is
/// skipped; use `jackin eject <selector> --purge` for those.
pub fn prune_instances(paths: &JackinPaths, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
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
        match purge_container_filesystem(paths, &container_base, runner) {
            Ok(()) => removed.push(container_base),
            Err(error) => skipped.push((container_base, error)),
        }
    }

    if !removed.is_empty() {
        // Stale index entries with no state dir are harmless — purge_container_filesystem
        // tolerates NotFound, so the next prune run retries unless the index is corrupt.
        let refs: Vec<&str> = removed.iter().map(String::as_str).collect();
        if let Err(err) = InstanceIndex::remove_many(&paths.data_dir, &refs) {
            eprintln!(
                "{} instance index could not be updated: {err:#}; run `jackin prune instances` again to retry",
                "warning:".yellow().bold()
            );
        }
        println!("Pruned {} instance(s):", removed.len());
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

fn ensure_role_resources_absent_for_purge(
    runner: &mut impl CommandRunner,
    container_name: &str,
) -> anyhow::Result<()> {
    ensure_container_absent_for_purge(runner, container_name, "role container")?;
    ensure_container_absent_for_purge(runner, &format!("{container_name}-dind"), "DinD sidecar")
}

fn ensure_container_absent_for_purge(
    runner: &mut impl CommandRunner,
    container_name: &str,
    resource_label: &str,
) -> anyhow::Result<()> {
    let state_phrase = match super::attach::inspect_container_state(runner, container_name) {
        super::attach::ContainerState::NotFound => return Ok(()),
        super::attach::ContainerState::Running => "and is running",
        super::attach::ContainerState::Stopped { .. } => "but is stopped",
        super::attach::ContainerState::InspectUnavailable(reason) => {
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
    use crate::paths::JackinPaths;
    use crate::selector::RoleSelector;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    #[test]
    fn eject_all_targets_only_requested_class_family() {
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

    #[test]
    fn purge_all_removes_matching_state_directories() {
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
                image_tag: "jk-agent-smith",
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
                image_tag: "jk-agent-smith",
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

        let mut runner = FakeRunner::default();
        purge_class_data(&paths, &selector, &mut runner).unwrap();

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

    #[test]
    fn purge_container_state_refuses_when_role_container_exists() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-agent-smith";
        std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
        let mut runner = FakeRunner::with_capture_queue(["false 0 false".to_string()]);

        let err = purge_container_state(&paths, container, &mut runner).unwrap_err();

        assert!(
            err.to_string().contains("still exists but is stopped"),
            "got: {err}"
        );
        assert!(err.to_string().contains("jackin eject"), "got: {err}");
        assert!(paths.data_dir.join(container).exists());
    }

    #[test]
    fn purge_container_state_refuses_when_dind_sidecar_exists() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-agent-smith";
        std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
        let mut runner =
            FakeRunner::with_capture_queue([String::new(), "true 0 false".to_string()]);

        let err = purge_container_state(&paths, container, &mut runner).unwrap_err();

        assert!(err.to_string().contains("DinD sidecar"), "got: {err}");
        assert!(
            err.to_string().contains("still exists and is running"),
            "got: {err}"
        );
        assert!(paths.data_dir.join(container).exists());
    }

    #[test]
    fn eject_agent_removes_container_dind_and_network() {
        let mut runner = FakeRunner::default();

        eject_role("jk-agent-smith", &mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker rm -f jk-agent-smith",
                "docker rm -f jk-agent-smith-dind",
                "docker volume rm jk-agent-smith-dind-certs",
                "docker network rm jk-agent-smith-net",
            ]
        );
    }

    #[test]
    fn eject_agent_ignores_missing_runtime_resources() {
        let mut runner = FakeRunner {
            fail_with: vec![
                (
                    "docker rm -f jk-agent-smith".to_string(),
                    "Error response from daemon: No such container: jk-agent-smith".to_string(),
                ),
                (
                    "docker rm -f jk-agent-smith-dind".to_string(),
                    "Error response from daemon: No such container: jk-agent-smith-dind"
                        .to_string(),
                ),
                (
                    "docker volume rm jk-agent-smith-dind-certs".to_string(),
                    "Error response from daemon: No such volume: jk-agent-smith-dind-certs"
                        .to_string(),
                ),
                (
                    "docker network rm jk-agent-smith-net".to_string(),
                    "Error response from daemon: No such network: jk-agent-smith-net".to_string(),
                ),
            ],
            ..Default::default()
        };

        eject_role("jk-agent-smith", &mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker rm -f jk-agent-smith",
                "docker rm -f jk-agent-smith-dind",
                "docker volume rm jk-agent-smith-dind-certs",
                "docker network rm jk-agent-smith-net",
            ]
        );
    }

    #[test]
    fn exile_all_ejects_all_managed_agents() {
        let mut runner = FakeRunner::with_capture_queue([r"jk-k7p9m2xq-agentsmith
jk-a1b2c3d4-myworkspace-agentsmith"
            .to_string()]);

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.kind=role --format {{.Names}}",
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

    #[test]
    fn exile_all_continues_when_some_runtime_resources_are_missing() {
        let mut runner = FakeRunner {
            fail_with: vec![
                (
                    "docker rm -f jk-k7p9m2xq-agentsmith".to_string(),
                    "Error response from daemon: No such container: jk-k7p9m2xq-agentsmith"
                        .to_string(),
                ),
                (
                    "docker network rm jk-k7p9m2xq-agentsmith-net".to_string(),
                    "Error response from daemon: No such network: jk-k7p9m2xq-agentsmith-net"
                        .to_string(),
                ),
            ],
            capture_queue: VecDeque::from(vec![
                r"jk-k7p9m2xq-agentsmith
jk-a1b2c3d4-myworkspace-agentsmith"
                    .to_string(),
            ]),
            ..Default::default()
        };

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.kind=role --format {{.Names}}",
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

    #[test]
    fn is_missing_cleanup_error_tolerates_all_resource_types() {
        let container_err =
            anyhow::anyhow!("Error response from daemon: No such container: jk-agent-smith");
        let volume_err = anyhow::anyhow!(
            "Error response from daemon: No such volume: jk-agent-smith-dind-certs"
        );
        let network_err =
            anyhow::anyhow!("Error response from daemon: No such network: jk-agent-smith-net");
        let real_err = anyhow::anyhow!("Error response from daemon: permission denied");

        assert!(is_missing_cleanup_error(&container_err));
        assert!(is_missing_cleanup_error(&volume_err));
        assert!(is_missing_cleanup_error(&network_err));
        assert!(!is_missing_cleanup_error(&real_err));
    }

    #[test]
    fn gc_removes_orphaned_dind_and_network() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: docker ps -a --filter label=jackin.kind=dind
            "jk-agent-smith-dind\tjk-agent-smith".to_string(),
            // collect_orphaned_dind: list_role_names (running)
            String::new(),
            // gc_orphaned_networks: docker network ls
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jk-agent-smith"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jk-agent-smith-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jk-agent-smith-net"))
        );
    }

    #[test]
    fn gc_skips_dind_when_agent_is_running() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: docker ps -a --filter label=jackin.kind=dind
            "jk-agent-smith-dind\tjk-agent-smith".to_string(),
            // collect_orphaned_dind: running agent-labeled roles — role IS running
            "jk-agent-smith".to_string(),
            // gc_orphaned_networks: docker network ls
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
        );
    }

    #[test]
    fn gc_does_nothing_when_no_orphans() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: no DinD sidecars
            String::new(),
            // gc_orphaned_networks: no networks
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(!runner.recorded.iter().any(|c| c.contains("docker rm")));
    }

    #[test]
    fn gc_removes_orphaned_network_without_dind() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: no DinD sidecars
            String::new(),
            // gc_orphaned_networks: docker network ls — has a network
            "jk-agent-smith-net\tjk-agent-smith".to_string(),
            // gc_orphaned_networks: list_role_names (running) — role not running
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jk-agent-smith-net"))
        );
    }

    #[test]
    fn gc_cleans_multiple_orphans() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: two orphaned DinD sidecars
            "jk-agent-smith-dind\tjk-agent-smith\njk-neo-dind\tjk-neo".to_string(),
            // collect_orphaned_dind: list_role_names (running)
            String::new(),
            // gc_orphaned_networks: no additional networks
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jk-agent-smith-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jk-neo-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jk-neo-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jk-neo-net"))
        );
    }

    // ── prune_dir ────────────────────────────────────────────────────────────

    #[test]
    fn prune_dir_removes_existing_directory() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cache");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("file.txt"), b"data").unwrap();

        prune_dir(&target, "cache").unwrap();

        assert!(!target.exists());
    }

    #[test]
    fn prune_dir_is_ok_when_directory_absent() {
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
                image_tag: "jk-agent-smith",
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

    #[test]
    fn prune_instances_removes_terminal_statuses_only() {
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

        let mut runner = FakeRunner::default();
        prune_instances(&paths, &mut runner).unwrap();

        assert!(!paths.data_dir.join(prunable).exists());
        assert!(paths.data_dir.join(kept).exists());
        let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert!(index.instances.iter().all(|e| e.container_base != prunable));
        assert!(index.instances.iter().any(|e| e.container_base == kept));
    }

    #[test]
    fn prune_instances_skips_when_docker_resources_present() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-k7p9m2xq-agentsmith";
        make_instance_at(
            &paths,
            container,
            crate::instance::InstanceStatus::CleanExited,
        );

        // Fake runner returns non-empty inspect → container still exists.
        let mut runner = FakeRunner::with_capture_queue(["false 0 false".to_string()]);
        prune_instances(&paths, &mut runner).unwrap();

        assert!(paths.data_dir.join(container).exists());
        let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert!(
            index
                .instances
                .iter()
                .any(|e| e.container_base == container)
        );
    }

    #[test]
    fn prune_instances_is_ok_when_data_dir_absent() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        let mut runner = FakeRunner::default();
        prune_instances(&paths, &mut runner).unwrap();
    }

    // ── prune_images ─────────────────────────────────────────────────────────

    #[test]
    fn prune_images_skips_images_in_use_by_role_containers() {
        let mut runner = FakeRunner::with_capture_queue([
            "jk-agent-smith:latest".to_string(), // docker images output
            "jk-agent-smith".to_string(),        // docker ps -a output (no :tag)
        ]);

        prune_images(&mut runner).unwrap();

        assert!(!runner.recorded.iter().any(|c| c.contains("docker rmi")));
    }

    #[test]
    fn prune_images_counts_rmi_in_use_error_as_skipped_not_failed() {
        // Image passes the pre-filter (not in the in_use set from docker ps)
        // but docker rmi returns an in-use error at removal time. Should be
        // skipped (Ok), not failed (error message + nonzero failed count).
        let mut runner = FakeRunner {
            fail_with: vec![(
                "docker rmi jk-agent-smith:latest".to_string(),
                "conflict: unable to remove (cannot be forced) - image is being used by running container"
                    .to_string(),
            )],
            capture_queue: std::collections::VecDeque::from(vec![
                "jk-agent-smith:latest".to_string(), // docker images
                String::new(),                        // docker ps -a: no containers in index
            ]),
            ..Default::default()
        };

        prune_images(&mut runner).unwrap();

        // rmi was attempted (image was not in the pre-filter set)
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rmi jk-agent-smith:latest"))
        );
    }

    #[test]
    fn prune_images_removes_images_not_in_use() {
        let mut runner = FakeRunner::with_capture_queue([
            "jk-agent-smith:latest".to_string(), // docker images output
            String::new(),                       // docker ps -a: no containers
        ]);

        prune_images(&mut runner).unwrap();

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rmi jk-agent-smith:latest"))
        );
    }

    #[test]
    fn prune_images_is_ok_when_no_images_found() {
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        prune_images(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec!["docker images --filter reference=jk-* --format {{.Repository}}:{{.Tag}}"]
        );
    }

    #[test]
    fn prune_images_is_ok_when_rmi_fails_with_real_error() {
        // A real Docker error (not in-use, not missing) is printed to stderr
        // but prune_images still returns Ok — best-effort cleanup.
        let mut runner = FakeRunner {
            fail_with: vec![(
                "docker rmi jk-agent-smith:latest".to_string(),
                "Error response from daemon: permission denied".to_string(),
            )],
            capture_queue: std::collections::VecDeque::from(vec![
                "jk-agent-smith:latest".to_string(), // docker images
                String::new(),                       // docker ps -a
            ]),
            ..Default::default()
        };

        prune_images(&mut runner).unwrap();

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rmi jk-agent-smith:latest"))
        );
    }

    #[test]
    fn prune_images_mixed_removed_and_skipped() {
        // One image is in-use (pre-filtered), one is removed successfully.
        let mut runner = FakeRunner::with_capture_queue([
            "jk-agent-smith:latest\njk-neo:latest".to_string(), // docker images: two images
            "jk-neo".to_string(), // docker ps -a: jk-neo in use (no :tag → normalised to jk-neo:latest)
        ]);

        prune_images(&mut runner).unwrap();

        // Only jk-agent-smith:latest should have had rmi attempted.
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rmi jk-agent-smith:latest"))
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rmi jk-neo:latest"))
        );
    }

    #[test]
    fn prune_images_skips_when_image_disappears_between_list_and_rmi() {
        // TOCTOU: image listed but already gone by rmi time — should be skipped, not failed.
        let mut runner = FakeRunner {
            fail_with: vec![(
                "docker rmi jk-agent-smith:latest".to_string(),
                "Error response from daemon: No such image: jk-agent-smith:latest".to_string(),
            )],
            capture_queue: std::collections::VecDeque::from(vec![
                "jk-agent-smith:latest".to_string(),
                String::new(),
            ]),
            ..Default::default()
        };

        prune_images(&mut runner).unwrap();
    }

    #[test]
    fn prune_instances_removes_all_four_prunable_statuses() {
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

        let mut runner = FakeRunner::default();
        prune_instances(&paths, &mut runner).unwrap();

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

    #[test]
    fn prune_dir_returns_err_with_path_context_on_failure() {
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
}
