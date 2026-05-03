use crate::docker::CommandRunner;
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use owo_colors::OwoColorize;

use super::discovery::{capture_managed_container_rows, list_managed_role_names, list_role_names};
use super::naming::{FILTER_KIND_DIND, FILTER_MANAGED, dind_certs_volume};

pub fn purge_class_data(paths: &JackinPaths, selector: &RoleSelector) -> anyhow::Result<()> {
    if !paths.data_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&paths.data_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if crate::instance::class_family_matches(selector, &file_name) {
            std::fs::remove_dir_all(entry.path())?;
        }
    }

    Ok(())
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

fn is_missing_cleanup_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("No such container")
        || message.contains("No such volume")
        || message.contains("No such network")
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

fn collect_legacy_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let output = capture_managed_container_rows(
        runner,
        true,
        "{{.Names}}\t{{.Label \"jackin.role\"}}\t{{.Label \"jackin.kind\"}}",
    )?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let name = parts.next()?;
            let role = parts.next().unwrap_or("");
            let kind = parts.next().unwrap_or("");
            if name.is_empty() || role.is_empty() || !kind.is_empty() {
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
    let mut sidecars = collect_labeled_dind(runner)?;
    sidecars.extend(collect_legacy_dind(runner)?);

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

        // Remove stopped role container, DinD sidecar, cert volume, and network.
        let _ = run_cleanup_command(runner, &["rm", "-f", &info.role]);
        let _ = run_cleanup_command(runner, &["rm", "-f", &info.name]);
        let _ = run_cleanup_command(runner, &["volume", "rm", &certs_volume]);
        let _ = run_cleanup_command(runner, &["network", "rm", &network]);

        eprintln!(
            "        {} orphaned resources for {}",
            "cleaned up".dimmed(),
            info.role
        );
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
        let _ = run_cleanup_command(runner, &["network", "rm", net_name]);
    }
}

pub fn exile_all(runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let names = list_managed_role_names(runner)?;
    for name in names {
        eject_role(&name, runner)?;
    }
    Ok(())
}

/// Refuse to proceed if the named role's container is currently running.
/// Used by purge to close a pre-existing gap (also relevant to shared mode).
pub fn ensure_role_not_running(
    runner: &mut impl CommandRunner,
    short_name: &str,
) -> anyhow::Result<()> {
    let running = list_role_names(runner, false)?;
    let container = format!("jackin-{short_name}");
    if running.iter().any(|n| n == &container || n == short_name) {
        anyhow::bail!(
            "role `{short_name}` is currently running; run `jackin eject {short_name}` first \
             (or `jackin eject {short_name} --purge` to combine eject and purge)"
        );
    }
    Ok(())
}

#[cfg(test)]
mod purge_guard_tests {
    use super::*;
    use crate::runtime::test_support::FakeRunner;

    #[test]
    fn purge_refuses_when_container_running() {
        let mut runner = FakeRunner::default();
        // list_role_names performs two `docker ps` queries (agent-labeled
        // first, then a legacy fallback).  The first response includes the
        // running container; the second can stay empty.
        runner
            .capture_queue
            .push_back("jackin-the-architect\n".into());
        runner.capture_queue.push_back(String::new());
        let err = ensure_role_not_running(&mut runner, "the-architect").unwrap_err();
        assert!(
            err.to_string().contains("running"),
            "error did not mention 'running': {err}"
        );
        assert!(
            err.to_string().contains("jackin eject"),
            "error did not mention 'jackin eject': {err}"
        );
    }

    #[test]
    fn purge_proceeds_when_container_not_running() {
        let mut runner = FakeRunner::default();
        runner.capture_queue.push_back(String::new());
        runner.capture_queue.push_back(String::new());
        ensure_role_not_running(&mut runner, "the-architect").unwrap();
    }
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
            "jackin-agent-smith".to_string(),
            "jackin-agent-smith-clone-1".to_string(),
            "jackin-chainargos-the-architect".to_string(),
        ];

        let matched = matching_family(&selector, &names);

        assert_eq!(
            matched,
            vec!["jackin-agent-smith", "jackin-agent-smith-clone-1"]
        );
    }

    #[test]
    fn purge_all_removes_matching_state_directories() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(paths.data_dir.join("jackin-agent-smith")).unwrap();
        std::fs::create_dir_all(paths.data_dir.join("jackin-agent-smith-clone-1")).unwrap();
        std::fs::create_dir_all(paths.data_dir.join("jackin-chainargos-the-architect")).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");

        purge_class_data(&paths, &selector).unwrap();

        assert!(!paths.data_dir.join("jackin-agent-smith").exists());
        assert!(!paths.data_dir.join("jackin-agent-smith-clone-1").exists());
        assert!(
            paths
                .data_dir
                .join("jackin-chainargos-the-architect")
                .exists()
        );
    }

    #[test]
    fn eject_agent_removes_container_dind_and_network() {
        let mut runner = FakeRunner::default();

        eject_role("jackin-agent-smith", &mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker volume rm jackin-agent-smith-dind-certs",
                "docker network rm jackin-agent-smith-net",
            ]
        );
    }

    #[test]
    fn eject_agent_ignores_missing_runtime_resources() {
        let mut runner = FakeRunner {
            fail_with: vec![
                (
                    "docker rm -f jackin-agent-smith".to_string(),
                    "Error response from daemon: No such container: jackin-agent-smith".to_string(),
                ),
                (
                    "docker rm -f jackin-agent-smith-dind".to_string(),
                    "Error response from daemon: No such container: jackin-agent-smith-dind"
                        .to_string(),
                ),
                (
                    "docker volume rm jackin-agent-smith-dind-certs".to_string(),
                    "Error response from daemon: No such volume: jackin-agent-smith-dind-certs"
                        .to_string(),
                ),
                (
                    "docker network rm jackin-agent-smith-net".to_string(),
                    "Error response from daemon: No such network: jackin-agent-smith-net"
                        .to_string(),
                ),
            ],
            ..Default::default()
        };

        eject_role("jackin-agent-smith", &mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker volume rm jackin-agent-smith-dind-certs",
                "docker network rm jackin-agent-smith-net",
            ]
        );
    }

    #[test]
    fn exile_all_ejects_all_managed_agents() {
        let mut runner = FakeRunner::with_capture_queue([
            r"jackin-agent-smith
jackin-agent-smith-clone-1"
                .to_string(),
            String::new(),
        ]);

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.kind=role --format {{.Names}}",
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.role\"}}\t{{.Label \"jackin.kind\"}}",
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker volume rm jackin-agent-smith-dind-certs",
                "docker network rm jackin-agent-smith-net",
                "docker rm -f jackin-agent-smith-clone-1",
                "docker rm -f jackin-agent-smith-clone-1-dind",
                "docker volume rm jackin-agent-smith-clone-1-dind-certs",
                "docker network rm jackin-agent-smith-clone-1-net",
            ]
        );
    }

    #[test]
    fn exile_all_continues_when_some_runtime_resources_are_missing() {
        let mut runner = FakeRunner {
            fail_with: vec![
                (
                    "docker rm -f jackin-agent-smith".to_string(),
                    "Error response from daemon: No such container: jackin-agent-smith".to_string(),
                ),
                (
                    "docker network rm jackin-agent-smith-net".to_string(),
                    "Error response from daemon: No such network: jackin-agent-smith-net"
                        .to_string(),
                ),
            ],
            capture_queue: VecDeque::from(vec![
                r"jackin-agent-smith
jackin-agent-smith-clone-1"
                    .to_string(),
                String::new(),
            ]),
            ..Default::default()
        };

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.kind=role --format {{.Names}}",
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.role\"}}\t{{.Label \"jackin.kind\"}}",
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker volume rm jackin-agent-smith-dind-certs",
                "docker network rm jackin-agent-smith-net",
                "docker rm -f jackin-agent-smith-clone-1",
                "docker rm -f jackin-agent-smith-clone-1-dind",
                "docker volume rm jackin-agent-smith-clone-1-dind-certs",
                "docker network rm jackin-agent-smith-clone-1-net",
            ]
        );
    }

    #[test]
    fn is_missing_cleanup_error_tolerates_all_resource_types() {
        let container_err =
            anyhow::anyhow!("Error response from daemon: No such container: jackin-agent-smith");
        let volume_err = anyhow::anyhow!(
            "Error response from daemon: No such volume: jackin-agent-smith-dind-certs"
        );
        let network_err =
            anyhow::anyhow!("Error response from daemon: No such network: jackin-agent-smith-net");
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
            "jackin-agent-smith-dind\tjackin-agent-smith".to_string(),
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
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jackin-agent-smith-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jackin-agent-smith-net"))
        );
    }

    #[test]
    fn gc_skips_dind_when_agent_is_running() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: docker ps -a --filter label=jackin.kind=dind
            "jackin-agent-smith-dind\tjackin-agent-smith".to_string(),
            // collect_orphaned_dind: legacy managed sidecars without role labels
            String::new(),
            // collect_orphaned_dind: running agent-labeled roles — role IS running
            "jackin-agent-smith".to_string(),
            // collect_orphaned_dind: running legacy roles without role labels
            String::new(),
            // gc_orphaned_networks: docker network ls
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
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
            "jackin-agent-smith-net\tjackin-agent-smith".to_string(),
            // gc_orphaned_networks: list_role_names (running) — role not running
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jackin-agent-smith-net"))
        );
    }

    #[test]
    fn gc_cleans_multiple_orphans() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: two orphaned DinD sidecars
            "jackin-agent-smith-dind\tjackin-agent-smith\njackin-neo-dind\tjackin-neo".to_string(),
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
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jackin-agent-smith-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-neo-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jackin-neo-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jackin-neo-net"))
        );
    }

    #[test]
    fn gc_removes_legacy_orphaned_dind_without_role_label() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: agent-labeled DinD sidecars
            String::new(),
            // collect_orphaned_dind: legacy managed sidecars without role labels
            "jackin-agent-smith-dind\tjackin-agent-smith\t".to_string(),
            // collect_orphaned_dind: running agent-labeled roles
            String::new(),
            // collect_orphaned_dind: running legacy roles without role labels
            String::new(),
            // gc_orphaned_networks: no additional networks
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith"))
        );
    }
}
