use crate::docker::{CommandRunner, RunOptions};
use crate::instance::{InstanceIndex, InstanceManifest, InstanceStatus};
use crate::paths::JackinPaths;
use crate::tui;

use super::naming::{LABEL_KIND_DIND, LABEL_MANAGED, dind_certs_volume};

#[derive(Debug, PartialEq, Eq)]
pub enum ContainerState {
    /// `docker inspect` confirmed the named container does not exist.
    NotFound,
    /// `docker inspect` could not determine container state.
    InspectUnavailable(String),
    Running,
    Stopped {
        exit_code: i32,
        oom_killed: bool,
    },
}

/// Query a container's state with a single `docker inspect` call.
///
/// Uses Go-template formatting to extract three fields in one round trip:
/// `Running`, `ExitCode`, and `OOMKilled`.
pub fn inspect_container_state(runner: &mut impl CommandRunner, name: &str) -> ContainerState {
    let output = match runner.capture(
        "docker",
        &[
            "inspect",
            "--format",
            "{{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}}",
            name,
        ],
        None,
    ) {
        Ok(output) => output,
        Err(error) => {
            let error = error.to_string();
            if docker_inspect_reports_missing_container(&error) {
                return ContainerState::NotFound;
            }
            return ContainerState::InspectUnavailable(error);
        }
    };

    let mut parts = output.split_whitespace();
    let Some(running) = parts.next() else {
        return ContainerState::NotFound;
    };
    if running == "true" {
        return ContainerState::Running;
    }
    if running != "false" {
        return ContainerState::InspectUnavailable(format!(
            "unexpected docker inspect output for '{name}': {output:?}"
        ));
    }
    let Some(exit_code) = parts.next().and_then(|s| s.parse().ok()) else {
        return ContainerState::InspectUnavailable(format!(
            "unexpected docker inspect output for '{name}': {output:?}"
        ));
    };
    let Some(oom_killed) = parts.next().map(|part| part == "true") else {
        return ContainerState::InspectUnavailable(format!(
            "unexpected docker inspect output for '{name}': {output:?}"
        ));
    };
    ContainerState::Stopped {
        exit_code,
        oom_killed,
    }
}

fn docker_inspect_reports_missing_container(error: &str) -> bool {
    docker_inspect_reports_missing_resource(error)
}

fn docker_inspect_reports_missing_resource(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("no such object")
        || error.contains("no such container")
        || error.contains("no such network")
}

fn inspect_unavailable_message(container_name: &str, reason: &str) -> String {
    format!(
        "cannot inspect container '{container_name}' because Docker is unavailable or returned an unexpected response: {reason}"
    )
}

/// Re-attach to a running role, or restart a crashed one in place.
///
/// Behavior by container state:
///   - `Running`                  → attach directly.
///   - `Stopped` / exit 0         → error.  The previous session ended cleanly;
///     the user wants `jackin load` for a new one.
///   - `Stopped` / exit ≠0 or OOM → ensure the derived `DinD` sidecar is
///     ready, restart the existing container, then attach.
///   - `NotFound`                 → error.
pub(super) fn attach_running(
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    runner.run(
        "docker",
        &[
            "attach",
            "--detach-keys=",
            "--sig-proxy=false",
            container_name,
        ],
        None,
        &RunOptions::default(),
    )
}

pub fn hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    // Reconcile keep_awake right before each `attach_running` call.
    // `attach_running` blocks on `docker attach` until the container
    // exits, so the post-hardline reconcile in `app::Command::Hardline`
    // would fire too late: by the time attach returns, the container
    // is stopped and the keep_awake count is zero. Firing here, while
    // the container is observably running, ensures caffeinate spawns
    // for the duration of the re-attached session.
    let attach_outcome = match inspect_container_state(runner, container_name) {
        ContainerState::Running => {
            super::caffeinate::reconcile(paths, runner);
            attach_running(container_name, runner)
        }
        ContainerState::NotFound => {
            if let Some(message) = missing_restore_message(paths, container_name)? {
                anyhow::bail!("{message}");
            }
            anyhow::bail!(
                "container '{container_name}' not found; use `jackin load` to start a new session"
            )
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!("{}", inspect_unavailable_message(container_name, &reason))
        }
        ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        } => {
            anyhow::bail!(
                "container '{container_name}' exited cleanly; \
                 use `jackin load` to start a new session"
            )
        }
        ContainerState::Stopped {
            exit_code,
            oom_killed,
        } => {
            let dind = format!("{container_name}-dind");
            match inspect_container_state(runner, &dind) {
                ContainerState::Running => {}
                ContainerState::NotFound => {
                    restore_missing_dind_sidecar(container_name, &dind, runner)?;
                }
                ContainerState::InspectUnavailable(reason) => {
                    anyhow::bail!("{}", inspect_unavailable_message(&dind, &reason))
                }
                ContainerState::Stopped { .. } => {
                    eprintln!("Restarting stopped DinD sidecar '{dind}'...");
                    runner.run("docker", &["start", &dind], None, &RunOptions::default())?;
                    let certs_volume = format!("{container_name}-dind-certs");
                    wait_for_dind(&dind, &certs_volume, runner, false)?;
                }
            }
            let reason = if oom_killed {
                "OOM killed".to_string()
            } else {
                format!("exit {exit_code}")
            };
            eprintln!("Restarting crashed container '{container_name}' ({reason})\u{2026}");
            runner.run(
                "docker",
                &["start", container_name],
                None,
                &RunOptions::default(),
            )?;
            super::caffeinate::reconcile(paths, runner);
            attach_running(container_name, runner)
        }
    };
    attach_outcome?;

    // Finalize per-mount isolation worktrees after re-attach. We do not honor
    // a `ReturnToAgent` decision here — `hardline` is itself a re-attach, and
    // the operator can simply re-invoke `jackin hardline` to come back.
    let outcome = crate::runtime::launch::inspect_attach_outcome(runner, container_name)?;
    super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let mut prompt = crate::isolation::finalize::StdinPrompt;
    let _ = crate::isolation::finalize::finalize_foreground_session(
        container_name,
        &paths.data_dir.join(container_name),
        outcome,
        interactive,
        &mut prompt,
        runner,
    )?;
    Ok(())
}

pub fn inspect_hardline_instance(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<String> {
    let state_dir = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read(&state_dir).ok();
    let dind_name = manifest.as_ref().map_or_else(
        || format!("{container_name}-dind"),
        |manifest| manifest.docker.dind_container.clone(),
    );
    let network_name = manifest.as_ref().map_or_else(
        || format!("{container_name}-net"),
        |manifest| manifest.docker.network.clone(),
    );
    let certs_volume = manifest.as_ref().map_or_else(
        || dind_certs_volume(container_name),
        |manifest| manifest.docker.certs_volume.clone(),
    );

    let role_state = describe_container_state(inspect_container_state(runner, container_name));
    let dind_state = describe_container_state(inspect_container_state(runner, &dind_name));
    let network_state = describe_network_state(inspect_docker_network(runner, &network_name));
    let mounts = describe_mount_state(&state_dir);

    let mut lines = vec![
        format!("Instance: {container_name}"),
        format!("State directory: {}", state_dir.display()),
    ];
    if let Some(manifest) = manifest {
        lines.extend([
            format!("Instance ID: {}", manifest.instance_id),
            format!("Workspace: {}", manifest.workspace_label),
            format!("Role: {}", manifest.role_key),
            format!("Agent: {}", manifest.agent_runtime),
            format!("Status: {}", describe_instance_status(manifest.status)),
            format!("Updated: {}", manifest.updated_at),
        ]);
        if let Some(outcome) = manifest.last_attach_outcome {
            lines.push(format!("Last attach outcome: {outcome}"));
        }
        if let Some(source_ref) = manifest.role_source_ref {
            lines.push(format!(
                "Role source: {} ({source_ref})",
                manifest.role_source_git
            ));
        } else if !manifest.role_source_git.is_empty() {
            lines.push(format!("Role source: {}", manifest.role_source_git));
        }
    } else {
        lines.push("Manifest: missing".to_string());
    }

    lines.extend([
        format!("Role container: {container_name} ({role_state})"),
        format!("DinD container: {dind_name} ({dind_state})"),
        format!("Docker network: {network_name} ({network_state})"),
        format!("DinD cert volume: {certs_volume}"),
        format!("Mounts: {mounts}"),
    ]);
    Ok(lines.join("\n"))
}

fn describe_container_state(state: ContainerState) -> String {
    match state {
        ContainerState::Running => "running".to_string(),
        ContainerState::Stopped {
            exit_code,
            oom_killed: false,
        } => format!("stopped exit:{exit_code}"),
        ContainerState::Stopped {
            oom_killed: true, ..
        } => "stopped oom_killed".to_string(),
        ContainerState::NotFound => "missing".to_string(),
        ContainerState::InspectUnavailable(reason) => format!("unavailable: {reason}"),
    }
}

fn describe_network_state(state: DockerNetworkState) -> String {
    match state {
        DockerNetworkState::Present => "present".to_string(),
        DockerNetworkState::NotFound => "missing".to_string(),
        DockerNetworkState::InspectUnavailable(reason) => format!("unavailable: {reason}"),
    }
}

const fn describe_instance_status(status: InstanceStatus) -> &'static str {
    match status {
        InstanceStatus::Active => "active",
        InstanceStatus::Running => "running",
        InstanceStatus::CleanExited => "clean_exited",
        InstanceStatus::Crashed => "crashed",
        InstanceStatus::PreservedDirty => "preserved_dirty",
        InstanceStatus::PreservedUnpushed => "preserved_unpushed",
        InstanceStatus::RestoreAvailable => "restore_available",
        InstanceStatus::Superseded => "superseded",
        InstanceStatus::Purged => "purged",
        InstanceStatus::FailedSetup => "failed_setup",
    }
}

fn describe_mount_state(state_dir: &std::path::Path) -> String {
    let Ok(records) = crate::isolation::state::read_records(state_dir) else {
        return "unknown".to_string();
    };
    if records.is_empty() {
        return "none".to_string();
    }
    let dirty = records
        .iter()
        .filter(|record| {
            record.cleanup_status == crate::isolation::state::CleanupStatus::PreservedDirty
        })
        .count();
    let unpushed = records
        .iter()
        .filter(|record| {
            record.cleanup_status == crate::isolation::state::CleanupStatus::PreservedUnpushed
        })
        .count();
    if dirty > 0 || unpushed > 0 {
        return format!(
            "{} total, {dirty} dirty, {unpushed} unpushed",
            records.len()
        );
    }
    format!("{} total", records.len())
}

fn restore_missing_dind_sidecar(
    container_name: &str,
    dind: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let network = format!("{container_name}-net");
    let certs_volume = dind_certs_volume(container_name);
    let role_label = format!("jackin.role={container_name}");
    ensure_hardline_network(container_name, &network, &role_label, runner)?;

    eprintln!("Recreating missing DinD sidecar '{dind}'...");
    let certs_dind_mount = format!("{certs_volume}:/certs/client");
    let dind_tls_san = format!("DOCKER_TLS_SAN=DNS:{dind}");
    runner.run(
        "docker",
        &[
            "run",
            "-d",
            "--name",
            dind,
            "--network",
            &network,
            "--privileged",
            "--label",
            LABEL_MANAGED,
            "--label",
            LABEL_KIND_DIND,
            "--label",
            &role_label,
            "-e",
            "DOCKER_TLS_CERTDIR=/certs",
            "-e",
            &dind_tls_san,
            "-v",
            &certs_dind_mount,
            "docker:dind",
        ],
        None,
        &RunOptions::default(),
    )?;
    wait_for_dind(dind, &certs_volume, runner, false)
}

fn ensure_hardline_network(
    container_name: &str,
    network: &str,
    role_label: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    match inspect_docker_network(runner, network) {
        DockerNetworkState::Present => Ok(()),
        DockerNetworkState::InspectUnavailable(reason) => {
            anyhow::bail!(
                "cannot inspect Docker network '{network}' while rebuilding DinD sidecar: {reason}"
            );
        }
        DockerNetworkState::NotFound => {
            eprintln!("Recreating missing Docker network '{network}'...");
            runner.run(
                "docker",
                &[
                    "network",
                    "create",
                    "--label",
                    LABEL_MANAGED,
                    "--label",
                    role_label,
                    network,
                ],
                None,
                &RunOptions::default(),
            )?;
            runner.run(
                "docker",
                &["network", "connect", network, container_name],
                None,
                &RunOptions::default(),
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerNetworkState {
    Present,
    NotFound,
    InspectUnavailable(String),
}

fn inspect_docker_network(runner: &mut impl CommandRunner, network: &str) -> DockerNetworkState {
    match runner.capture("docker", &["network", "inspect", network], None) {
        Ok(_) => DockerNetworkState::Present,
        Err(error) => {
            let error = error.to_string();
            if docker_inspect_reports_missing_resource(&error) {
                DockerNetworkState::NotFound
            } else {
                DockerNetworkState::InspectUnavailable(error)
            }
        }
    }
}

fn missing_restore_message(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<Option<String>> {
    let state_dir = paths.data_dir.join(container_name);
    let Ok(mut manifest) = InstanceManifest::read(&state_dir) else {
        return Ok(None);
    };
    if !manifest.is_restore_candidate() {
        return Ok(None);
    }

    manifest.mark_status(InstanceStatus::RestoreAvailable);
    manifest.write(&state_dir)?;
    InstanceIndex::update_manifest(&paths.data_dir, &manifest)?;
    Ok(Some(format!(
        "container '{container_name}' is missing, but jackin-managed local state remains recoverable at {}. \
         Run `jackin load` from the matching workspace to rebuild it, or `jackin eject {container_name} --purge` \
         to discard it. Any changes written only to the deleted container's writable layer are gone.",
        state_dir.display()
    )))
}

pub(super) fn wait_for_dind(
    dind_name: &str,
    certs_volume: &str,
    runner: &mut impl CommandRunner,
    _debug: bool,
) -> anyhow::Result<()> {
    // Wait for the DinD daemon to become ready (TLS handshake included).
    tui::spin_wait(
        "Waiting for Docker-in-Docker to be ready",
        30,
        std::time::Duration::from_secs(1),
        || {
            runner
                .capture("docker", &["exec", dind_name, "docker", "info"], None)
                .map(|_| ())
        },
    )
    .map_err(|_| anyhow::anyhow!("timed out waiting for Docker-in-Docker sidecar {dind_name}"))?;

    // Verify TLS client certificates were generated on the shared volume.
    // The DinD entrypoint writes certs before starting dockerd, so this
    // should succeed immediately after `docker info` passes.
    runner
        .capture(
            "docker",
            &["exec", dind_name, "test", "-f", "/certs/client/ca.pem"],
            None,
        )
        .map_err(|_| {
            anyhow::anyhow!(
                "DinD TLS client certificates not found on volume {certs_volume} — \
                 the DinD sidecar may have started without generating certificates"
            )
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::FakeRunner;
    use super::*;
    use tempfile::TempDir;

    fn test_paths() -> (TempDir, JackinPaths) {
        let dir = TempDir::new().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        (dir, paths)
    }

    #[test]
    fn hardline_attaches_when_container_is_running() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);

        hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap();

        // The attach command must appear; the trailing inspect for the
        // finalizer is appended after.
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c == "docker attach --detach-keys= --sig-proxy=false jackin-agent-smith"),
            "expected docker attach in recorded commands"
        );
    }

    #[test]
    fn hardline_errors_when_container_not_found() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("not found"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }

    #[test]
    fn inspect_container_state_distinguishes_missing_container_from_docker_failure() {
        let mut missing = FakeRunner::default();
        missing.fail_with.push((
            "docker inspect".to_string(),
            "Error: No such object: jackin-agent-smith".to_string(),
        ));
        assert_eq!(
            inspect_container_state(&mut missing, "jackin-agent-smith"),
            ContainerState::NotFound
        );

        let mut unavailable = FakeRunner::default();
        unavailable.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));
        assert!(matches!(
            inspect_container_state(&mut unavailable, "jackin-agent-smith"),
            ContainerState::InspectUnavailable(reason)
                if reason.contains("Cannot connect to the Docker daemon")
        ));
    }

    #[test]
    fn hardline_errors_when_docker_inspect_is_unavailable() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::default();
        runner.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));

        let err = hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("Docker is unavailable"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }

    #[test]
    fn hardline_marks_missing_manifest_restore_available() {
        let (_tmp, paths) = test_paths();
        let container_name = "jackin-workspace-agentsmith-k7p9m2xq";
        let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jackin-agent-smith",
            docker: crate::instance::DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        manifest.mark_status(InstanceStatus::Crashed);
        let state_dir = paths.data_dir.join(container_name);
        manifest.write(&state_dir).unwrap();
        InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, container_name, &mut runner).unwrap_err();

        assert!(err.to_string().contains("state remains recoverable"));
        let manifest = InstanceManifest::read(&state_dir).unwrap();
        assert_eq!(manifest.status, InstanceStatus::RestoreAvailable);
        let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert_eq!(index.instances[0].status, InstanceStatus::RestoreAvailable);
    }

    #[test]
    fn inspect_hardline_instance_reports_state_without_attaching() {
        let (_tmp, paths) = test_paths();
        let container_name = "jackin-workspace-agentsmith-k7p9m2xq";
        let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Codex,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: Some("feature/role"),
            image_tag: "jackin-agent-smith",
            docker: crate::instance::DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        manifest.mark_status(InstanceStatus::PreservedDirty);
        manifest.last_attach_outcome = Some("exit:137".to_string());
        manifest
            .write(&paths.data_dir.join(container_name))
            .unwrap();
        let mut runner = FakeRunner::with_capture_queue([
            "true 0 false".to_string(),
            "false 137 false".to_string(),
            "[]".to_string(),
        ]);

        let report = inspect_hardline_instance(&paths, container_name, &mut runner).unwrap();

        assert!(report.contains("Instance ID: k7p9m2xq"), "{report}");
        assert!(report.contains("Workspace: workspace"), "{report}");
        assert!(report.contains("Role: agent-smith"), "{report}");
        assert!(report.contains("Agent: codex"), "{report}");
        assert!(report.contains("Status: preserved_dirty"), "{report}");
        assert!(report.contains("Last attach outcome: exit:137"), "{report}");
        assert!(report.contains("Role container: jackin-workspace-agentsmith-k7p9m2xq (running)"));
        assert!(report.contains(
            "DinD container: jackin-workspace-agentsmith-k7p9m2xq-dind (stopped exit:137)"
        ));
        assert!(
            report.contains("Docker network: jackin-workspace-agentsmith-k7p9m2xq-net (present)")
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }

    #[test]
    fn inspect_hardline_instance_still_reports_manifest_when_docker_unavailable() {
        let (_tmp, paths) = test_paths();
        let container_name = "jackin-workspace-agentsmith-k7p9m2xq";
        let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jackin-agent-smith",
            docker: crate::instance::DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        manifest
            .write(&paths.data_dir.join(container_name))
            .unwrap();
        let mut runner = FakeRunner::default();
        runner.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));
        runner.fail_with.push((
            "docker network inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));

        let report = inspect_hardline_instance(&paths, container_name, &mut runner).unwrap();

        assert!(report.contains("Workspace: workspace"), "{report}");
        assert!(
            report.contains("Role container: jackin-workspace-agentsmith-k7p9m2xq (unavailable:")
        );
        assert!(
            report
                .contains("Docker network: jackin-workspace-agentsmith-k7p9m2xq-net (unavailable:")
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }

    #[test]
    fn hardline_errors_on_clean_exit() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["false 0 false".to_string()]);

        let err = hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("exited cleanly"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }

    #[test]
    fn hardline_restarts_crashed_container_when_dind_running() {
        let (_tmp, paths) = test_paths();
        // Inspect calls: container stopped w/ exit 137, then dind running.
        let mut runner = FakeRunner::with_capture_queue([
            "false 137 false".to_string(),
            "true 0 false".to_string(),
        ]);

        hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap();

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c == "docker start jackin-agent-smith"),
            "expected docker start before attach"
        );
        let start_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker start jackin-agent-smith")
            .unwrap();
        let attach_idx = runner
            .recorded
            .iter()
            .position(|c| c.contains("docker attach"))
            .unwrap();
        assert!(start_idx < attach_idx, "start must precede attach");
    }

    #[test]
    fn hardline_recreates_missing_dind_and_network() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue([
            "false 137 false".to_string(),
            String::new(),
            String::new(),
        ]);
        runner.fail_with.push((
            "docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jackin-agent-smith-dind".to_string(),
            "Error: No such object: jackin-agent-smith-dind".to_string(),
        ));
        runner.fail_with.push((
            "docker network inspect jackin-agent-smith-net".to_string(),
            "Error: No such network: jackin-agent-smith-net".to_string(),
        ));

        hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap();

        let network_create_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker network create --label jackin.managed=true --label jackin.role=jackin-agent-smith jackin-agent-smith-net")
            .expect("expected missing network recreation");
        let network_connect_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker network connect jackin-agent-smith-net jackin-agent-smith")
            .expect("expected role container network reconnect");
        let dind_run_idx = runner
            .recorded
            .iter()
            .position(|c| {
                c.contains("docker run -d --name jackin-agent-smith-dind")
                    && c.contains("--network jackin-agent-smith-net")
                    && c.contains("DOCKER_TLS_SAN=DNS:jackin-agent-smith-dind")
                    && c.contains("jackin-agent-smith-dind-certs:/certs/client")
            })
            .expect("expected missing DinD sidecar recreation");
        let dind_ready_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker exec jackin-agent-smith-dind docker info")
            .expect("expected DinD readiness check");
        let role_start_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker start jackin-agent-smith")
            .expect("expected role restart");
        let attach_idx = runner
            .recorded
            .iter()
            .position(|c| c.contains("docker attach"))
            .expect("expected role attach");
        assert!(network_create_idx < network_connect_idx);
        assert!(network_connect_idx < dind_run_idx);
        assert!(dind_run_idx < dind_ready_idx);
        assert!(dind_ready_idx < role_start_idx);
        assert!(role_start_idx < attach_idx);
    }

    #[test]
    fn hardline_refuses_when_dind_inspect_is_unavailable() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["false 137 false".to_string()]);
        runner.fail_with.push((
            "docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jackin-agent-smith-dind".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));

        let err = hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("Docker is unavailable"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }

    #[test]
    fn hardline_restarts_dind_when_sidecar_is_stopped() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue([
            "false 137 false".to_string(),
            "false 0 false".to_string(),
            String::new(),
            String::new(),
        ]);

        hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap();

        let dind_start_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker start jackin-agent-smith-dind")
            .expect("expected stopped DinD sidecar restart");
        let dind_ready_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker exec jackin-agent-smith-dind docker info")
            .expect("expected DinD readiness check");
        let role_start_idx = runner
            .recorded
            .iter()
            .position(|c| c == "docker start jackin-agent-smith")
            .expect("expected role restart");
        let attach_idx = runner
            .recorded
            .iter()
            .position(|c| c.contains("docker attach"))
            .expect("expected role attach");
        assert!(dind_start_idx < dind_ready_idx);
        assert!(dind_ready_idx < role_start_idx);
        assert!(role_start_idx < attach_idx);
    }
}
