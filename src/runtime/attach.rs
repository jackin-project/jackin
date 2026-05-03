use crate::docker::{CommandRunner, RunOptions};
use crate::paths::JackinPaths;
use crate::tui;

use super::identity::try_capture;

#[derive(Debug, PartialEq, Eq)]
pub enum ContainerState {
    /// `docker inspect` failed — container does not exist (or daemon is down).
    NotFound,
    Running,
    Stopped {
        exit_code: i32,
        oom_killed: bool,
    },
}

/// Query a container's state with a single `docker inspect` call.
///
/// Uses Go-template formatting to extract three fields in one round trip:
/// `Running`, `ExitCode`, and `OOMKilled`.  Returns `NotFound` when inspect
/// fails for any reason (missing container, daemon unreachable, parse error).
pub fn inspect_container_state(runner: &mut impl CommandRunner, name: &str) -> ContainerState {
    let Some(output) = try_capture(
        runner,
        "docker",
        &[
            "inspect",
            "--format",
            "{{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}}",
            name,
        ],
    ) else {
        return ContainerState::NotFound;
    };
    let mut parts = output.split_whitespace();
    let Some(running) = parts.next() else {
        return ContainerState::NotFound;
    };
    if running == "true" {
        return ContainerState::Running;
    }
    let exit_code: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let oom_killed = parts.next() == Some("true");
    ContainerState::Stopped {
        exit_code,
        oom_killed,
    }
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
            anyhow::bail!(
                "container '{container_name}' not found; use `jackin load` to start a new session"
            )
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
                    "DinD sidecar '{dind}' not found; use `jackin load` to rebuild the network"
                ),
                ContainerState::Stopped { .. } => anyhow::bail!(
                    "DinD sidecar '{dind}' is stopped; use `jackin load` to rebuild the network"
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
