use crate::docker::CommandRunner;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use owo_colors::OwoColorize;

use super::discovery::{
    capture_managed_container_rows, list_agent_names, list_managed_agent_names,
};
use super::naming::{FILTER_MANAGED, FILTER_ROLE_DIND, dind_certs_volume};

pub fn purge_class_data(paths: &JackinPaths, selector: &ClassSelector) -> anyhow::Result<()> {
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

pub fn eject_agent(container_name: &str, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
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
    agent: String,
}

fn collect_labeled_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let dind_output = runner.capture(
        "docker",
        &[
            "ps",
            "-a",
            "--filter",
            FILTER_ROLE_DIND,
            "--format",
            "{{.Names}}\t{{.Label \"jackin.agent\"}}",
        ],
        None,
    )?;

    Ok(dind_output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (name, agent) = line.split_once('\t')?;
            if agent.is_empty() {
                return None;
            }
            Some(DindInfo {
                name: name.to_string(),
                agent: agent.to_string(),
            })
        })
        .collect())
}

fn collect_legacy_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let output = capture_managed_container_rows(
        runner,
        true,
        "{{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
    )?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let name = parts.next()?;
            let agent = parts.next().unwrap_or("");
            let role = parts.next().unwrap_or("");
            if name.is_empty() || agent.is_empty() || !role.is_empty() {
                return None;
            }
            Some(DindInfo {
                name: name.to_string(),
                agent: agent.to_string(),
            })
        })
        .collect())
}

/// Return `DinD` sidecar containers whose corresponding agent container is no
/// longer running.  These are leftovers from hard kills, terminal closures,
/// or startup failures.
fn collect_orphaned_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let mut sidecars = collect_labeled_dind(runner)?;
    sidecars.extend(collect_legacy_dind(runner)?);

    if sidecars.is_empty() {
        return Ok(vec![]);
    }

    // Running agent containers (label filter excludes DinD sidecars).
    let running = list_agent_names(runner, false)?;

    Ok(sidecars
        .into_iter()
        .filter(|info| !running.contains(&info.agent))
        .collect())
}

/// Remove orphaned `DinD` containers, their associated agent containers, cert
/// volumes, and networks.  Errors are logged but do not abort the launch — GC
/// is best-effort.
pub(super) fn gc_orphaned_resources(runner: &mut impl CommandRunner) {
    let Ok(orphaned) = collect_orphaned_dind(runner) else {
        return;
    };

    for info in &orphaned {
        let certs_volume = dind_certs_volume(&info.agent);
        let network = format!("{}-net", info.agent);

        // Remove stopped agent container, DinD sidecar, cert volume, and network.
        let _ = run_cleanup_command(runner, &["rm", "-f", &info.agent]);
        let _ = run_cleanup_command(runner, &["rm", "-f", &info.name]);
        let _ = run_cleanup_command(runner, &["volume", "rm", &certs_volume]);
        let _ = run_cleanup_command(runner, &["network", "rm", &network]);

        eprintln!(
            "        {} orphaned resources for {}",
            "cleaned up".dimmed(),
            info.agent
        );
    }

    // Clean up any orphaned networks that survived without a DinD container
    // (e.g. the DinD container was manually removed but the network lingers).
    gc_orphaned_networks(runner);
}

/// Remove jackin-managed Docker networks whose owning agent container no
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
            "{{.Name}}\t{{.Label \"jackin.agent\"}}",
        ],
        None,
    ) else {
        return;
    };

    let networks: Vec<(&str, &str)> = net_output
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, agent)| !agent.is_empty())
        .collect();

    if networks.is_empty() {
        return;
    }

    let Ok(running) = list_agent_names(runner, false) else {
        return;
    };

    for (net_name, agent) in networks {
        if running.iter().any(|r| r == agent) {
            continue;
        }
        let _ = run_cleanup_command(runner, &["network", "rm", net_name]);
    }
}

pub fn exile_all(runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let names = list_managed_agent_names(runner)?;
    for name in names {
        eject_agent(&name, runner)?;
    }
    Ok(())
}

/// Refuse to proceed if the named agent's container is currently running.
/// Used by purge to close a pre-existing gap (also relevant to shared mode).
pub fn ensure_agent_not_running(
    runner: &mut impl CommandRunner,
    short_name: &str,
) -> anyhow::Result<()> {
    let running = list_agent_names(runner, false)?;
    let container = format!("jackin-{short_name}");
    if running.iter().any(|n| n == &container || n == short_name) {
        anyhow::bail!(
            "agent `{short_name}` is currently running; run `jackin eject {short_name}` first \
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
        // list_agent_names performs two `docker ps` queries (role-labeled
        // first, then a legacy fallback).  The first response includes the
        // running container; the second can stay empty.
        runner
            .capture_queue
            .push_back("jackin-the-architect\n".into());
        runner.capture_queue.push_back(String::new());
        let err = ensure_agent_not_running(&mut runner, "the-architect").unwrap_err();
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
        ensure_agent_not_running(&mut runner, "the-architect").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::super::naming::matching_family;
    use super::super::test_support::FakeRunner;
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    #[test]
    fn eject_all_targets_only_requested_class_family() {
        let selector = ClassSelector::new(None, "agent-smith");
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
        let selector = ClassSelector::new(None, "agent-smith");

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

        eject_agent("jackin-agent-smith", &mut runner).unwrap();

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

        eject_agent("jackin-agent-smith", &mut runner).unwrap();

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
                "docker ps -a --filter label=jackin.role=agent --format {{.Names}}",
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
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
                "docker ps -a --filter label=jackin.role=agent --format {{.Names}}",
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
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
            // collect_orphaned_dind: docker ps -a --filter label=jackin.role=dind
            "jackin-agent-smith-dind\tjackin-agent-smith".to_string(),
            // collect_orphaned_dind: list_agent_names (running)
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
            // collect_orphaned_dind: docker ps -a --filter label=jackin.role=dind
            "jackin-agent-smith-dind\tjackin-agent-smith".to_string(),
            // collect_orphaned_dind: legacy managed sidecars without role labels
            String::new(),
            // collect_orphaned_dind: running role-labeled agents — agent IS running
            "jackin-agent-smith".to_string(),
            // collect_orphaned_dind: running legacy agents without role labels
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
            // gc_orphaned_networks: list_agent_names (running) — agent not running
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
            // collect_orphaned_dind: list_agent_names (running)
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
            // collect_orphaned_dind: role-labeled DinD sidecars
            String::new(),
            // collect_orphaned_dind: legacy managed sidecars without role labels
            "jackin-agent-smith-dind\tjackin-agent-smith\t".to_string(),
            // collect_orphaned_dind: running role-labeled agents
            String::new(),
            // collect_orphaned_dind: running legacy agents without role labels
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
