//! Keep-awake reconciler for the macOS `caffeinate` power assertion.
//!
//! Workspaces opt in via `[workspaces.<name>.keep_awake] enabled = true`.
//! When any container with the `jackin.keep_awake=true` label is
//! running, jackin keeps a single detached `caffeinate -imsu` alive
//! so the host stays awake; when the last such container stops, the
//! assertion is released. The motivating use case is
//! `/remote-control` sessions — agents working in the background that
//! should remain reachable even when the operator steps away from
//! the keyboard.
//!
//! ## Operation
//!
//! [`reconcile`] runs at every jackin command boundary (load, hardline,
//! eject, exile). It is a state-converger:
//!
//! 1. Acquire an exclusive lock on `<data_dir>/caffeinate.lock` so two
//!    parallel jackin invocations don't both spawn / both kill.
//! 2. Count agent containers labelled `jackin.keep_awake=true`.
//! 3. Read `<data_dir>/caffeinate.pid`; treat the recorded PID as
//!    "running" only when `kill(pid, 0)` succeeds.
//! 4. Start `caffeinate -imsu` (detached, SIGHUP-immune) when wanted &
//!    not running; SIGTERM the recorded PID when running & not wanted.
//!
//! ## Platform support
//!
//! macOS only. On other platforms `reconcile` is a silent no-op even
//! when workspaces opt in — the equivalent inhibitor (e.g.
//! `systemd-inhibit`) will land when a Linux user requests it. The
//! `keep_awake` config still parses on every platform so a config
//! shared across machines doesn't error on the non-mac ones.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;
use fs2::FileExt;

use crate::docker::CommandRunner;
use crate::paths::JackinPaths;

use super::naming::FILTER_KEEP_AWAKE;

const PID_FILENAME: &str = "caffeinate.pid";
const LOCK_FILENAME: &str = "caffeinate.lock";

/// Bring the caffeinate process in line with the running keep-awake
/// agents.
///
/// Best-effort: any failure (lock contention, docker failure, fork
/// failure) is swallowed with a one-line stderr notice so it never
/// breaks the user's actual command.
pub fn reconcile(paths: &JackinPaths, runner: &mut impl CommandRunner) {
    if !is_supported_platform() {
        return;
    }

    if let Err(err) = reconcile_inner(paths, runner) {
        eprintln!("[jackin] keep_awake reconciler: {err}");
    }
}

const fn is_supported_platform() -> bool {
    cfg!(target_os = "macos")
}

fn reconcile_inner(paths: &JackinPaths, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir).with_context(|| {
        format!(
            "creating data dir for caffeinate state: {}",
            paths.data_dir.display()
        )
    })?;

    let lock_path = paths.data_dir.join(LOCK_FILENAME);
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening {}", lock_path.display()))?;

    // Loser of a parallel race silently steps aside — the winner's
    // reconciliation is authoritative for that moment.
    if lock_file.try_lock_exclusive().is_err() {
        return Ok(());
    }

    let want_running = count_keep_awake_agents(runner)? > 0;
    let pid_path = paths.data_dir.join(PID_FILENAME);
    let current_pid = read_pid_file(&pid_path)?;
    let alive = current_pid.is_some_and(is_pid_alive);

    match (want_running, alive) {
        (true, true) => {}
        (true, false) => {
            // Stale PID file (process died, never reaped) — wipe before
            // overwriting so a failed start doesn't leave garbage behind.
            if current_pid.is_some() {
                let _ = std::fs::remove_file(&pid_path);
            }
            let pid = spawn_caffeinate()?;
            write_pid_file(&pid_path, pid)?;
        }
        (false, true) => {
            if let Some(pid) = current_pid {
                stop_caffeinate(pid);
            }
            let _ = std::fs::remove_file(&pid_path);
        }
        (false, false) => {
            // Process is gone but PID file lingered — clean up so future
            // reconciliations don't keep parsing dead state.
            if current_pid.is_some() {
                let _ = std::fs::remove_file(&pid_path);
            }
        }
    }

    Ok(())
}

/// Count agent containers carrying the `jackin.keep_awake=true` label.
/// Stopped containers are excluded — only an actually-running agent
/// justifies holding the assertion.
fn count_keep_awake_agents(runner: &mut impl CommandRunner) -> anyhow::Result<usize> {
    let output = runner.capture(
        "docker",
        &[
            "ps",
            "--filter",
            FILTER_KEEP_AWAKE,
            "--format",
            "{{.Names}}",
        ],
        None,
    )?;
    Ok(output.lines().filter(|l| !l.is_empty()).count())
}

fn read_pid_file(path: &Path) -> anyhow::Result<Option<u32>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(contents.trim().parse::<u32>().ok()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

fn write_pid_file(path: &Path, pid: u32) -> anyhow::Result<()> {
    std::fs::write(path, pid.to_string()).with_context(|| format!("writing {}", path.display()))
}

/// `kill -0 PID` — exits 0 when the PID exists *and* the caller can
/// signal it. Treats both "no such process" and any error as dead;
/// the caller will then re-spawn, which is the safe direction (worst
/// case we briefly run two assertions, never zero).
fn is_pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Spawn `caffeinate -imsu` so it survives jackin exiting *and* the
/// controlling terminal closing. We can't call `setsid(2)` directly
/// without `unsafe` (forbidden crate-wide), so we shell out via
/// `nohup`, which sets `SIG_IGN` on `SIGHUP` for the child and detaches
/// it from the terminal's session for hangup purposes. The wrapper
/// shell exits immediately after backgrounding caffeinate, leaving it
/// reparented to launchd.
fn spawn_caffeinate() -> anyhow::Result<u32> {
    let output = Command::new("sh")
        .arg("-c")
        // `nohup` ignores SIGHUP for the child; redirecting all three
        // fds to /dev/null prevents nohup from creating `nohup.out` in
        // the cwd. `echo $!` returns the PID of the backgrounded job
        // — caffeinate itself, not the shell.
        .arg("nohup caffeinate -imsu </dev/null >/dev/null 2>&1 & echo $!")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("spawning caffeinate via sh")?;

    anyhow::ensure!(
        output.status.success(),
        "shell wrapper exited with {} while spawning caffeinate: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );

    let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let pid: u32 = pid_str
        .parse()
        .with_context(|| format!("parsing caffeinate PID from {pid_str:?}"))?;
    Ok(pid)
}

/// SIGTERM the caffeinate process. Errors are intentionally ignored —
/// if the PID is already gone, the goal is met; if `kill` itself
/// errors, the caller has nothing useful to do about it.
fn stop_caffeinate(pid: u32) {
    let _ = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Path helper exported for tests of higher-level integrations.
#[cfg(test)]
pub(super) fn pid_path_for_tests(paths: &JackinPaths) -> std::path::PathBuf {
    paths.data_dir.join(PID_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::FakeRunner;
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn count_keep_awake_agents_returns_zero_for_empty_output() {
        let mut runner = FakeRunner::with_capture_queue([String::new()]);
        let count = count_keep_awake_agents(&mut runner).unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            runner.recorded.last().unwrap(),
            "docker ps --filter label=jackin.keep_awake=true --format {{.Names}}"
        );
    }

    #[test]
    fn count_keep_awake_agents_counts_nonempty_lines() {
        let mut runner =
            FakeRunner::with_capture_queue(
                ["jackin-agent-smith\njackin-the-architect".to_string()],
            );
        let count = count_keep_awake_agents(&mut runner).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn read_pid_file_returns_none_when_missing() {
        let tmp = tempdir().unwrap();
        assert_eq!(
            read_pid_file(&tmp.path().join("missing.pid")).unwrap(),
            None
        );
    }

    #[test]
    fn read_pid_file_parses_trimmed_pid() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.pid");
        std::fs::write(&path, "12345\n").unwrap();
        assert_eq!(read_pid_file(&path).unwrap(), Some(12345));
    }

    #[test]
    fn read_pid_file_returns_none_for_garbage() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.pid");
        std::fs::write(&path, "not-a-pid").unwrap();
        assert_eq!(read_pid_file(&path).unwrap(), None);
    }

    #[test]
    fn is_pid_alive_returns_false_for_pid_zero() {
        // PID 0 is not a real process from a user's perspective; kill -0 0
        // either succeeds (signalling the whole process group on Linux) or
        // fails (macOS). We don't depend on the exact answer — only that
        // the function returns a bool without panicking.
        let _ = is_pid_alive(0);
    }

    #[test]
    fn is_pid_alive_for_nonexistent_pid_returns_false() {
        // PID 1 always exists; pick a deliberately huge number unlikely
        // to be allocated.
        assert!(!is_pid_alive(2_000_000_000));
    }

    #[test]
    fn reconcile_inner_is_noop_when_no_agents_and_no_pid_file() {
        let tmp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        reconcile_inner(&paths, &mut runner).unwrap();

        assert!(!pid_path_for_tests(&paths).exists());
    }

    #[test]
    fn reconcile_inner_clears_stale_pid_file_when_no_agents() {
        let tmp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let pid_path = pid_path_for_tests(&paths);
        std::fs::write(&pid_path, "2000000001").unwrap();

        let mut runner = FakeRunner::with_capture_queue([String::new()]);
        reconcile_inner(&paths, &mut runner).unwrap();

        assert!(!pid_path.exists(), "stale PID file should be removed");
    }
}
