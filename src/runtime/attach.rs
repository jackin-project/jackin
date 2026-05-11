use crate::docker::{CommandRunner, RunOptions};
use crate::instance::{InstanceIndex, InstanceManifest, InstanceStatus};
use crate::paths::JackinPaths;
use crate::tui;

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
    let error = error.to_ascii_lowercase();
    error.contains("no such object") || error.contains("no such container")
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
///   - `Stopped` / exit ≠0 or OOM → restart the existing container, then
///     attach, provided the `DinD` sidecar is still present and running.  If
///     `DinD` is gone or stopped, error — the network plumbing must be rebuilt
///     via `jackin load`.
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
                ContainerState::NotFound => anyhow::bail!(
                    "DinD sidecar '{dind}' not found; use `jackin load` to rebuild jackin-managed network state. \
                     The role container still exists, so jackin will not recreate it in place; any changes written \
                     only to that container's writable layer must be inspected from the existing container."
                ),
                ContainerState::InspectUnavailable(reason) => {
                    anyhow::bail!("{}", inspect_unavailable_message(&dind, &reason))
                }
                ContainerState::Stopped { .. } => anyhow::bail!(
                    "DinD sidecar '{dind}' is stopped; use `jackin load` to rebuild jackin-managed network state. \
                     The role container still exists, so jackin will not recreate it in place; any changes written \
                     only to that container's writable layer must be inspected from the existing container."
                ),
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
    fn hardline_refuses_when_dind_missing() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue([
            "false 137 false".to_string(),
            // Second inspect (DinD) returns empty → NotFound
            String::new(),
        ]);

        let err = hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("DinD sidecar"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
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
    fn hardline_refuses_when_dind_stopped() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue([
            "false 137 false".to_string(),
            "false 0 false".to_string(),
        ]);

        let err = hardline_agent(&paths, "jackin-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("stopped"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }
}
