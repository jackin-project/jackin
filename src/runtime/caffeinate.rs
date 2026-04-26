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
//! [`reconcile`] runs at every jackin command boundary (load, console,
//! hardline, eject, exile). It is a state-converger:
//!
//! 1. Acquire an exclusive lock on `<data_dir>/caffeinate.lock` so two
//!    parallel jackin invocations don't both spawn / both kill.
//! 2. Count agent containers labelled `jackin.keep_awake=true`.
//! 3. Read `<data_dir>/caffeinate.pid`; treat the recorded PID as
//!    "running" only when `ps -p <pid> -o comm=` reports `caffeinate`.
//!    Matching on the process basename (not just PID liveness via
//!    `kill -0`) closes the PID-reuse race where a recycled PID
//!    could otherwise look alive and cause SIGTERM of an unrelated
//!    user process.
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
    // reconciliation is authoritative for that moment. Genuine I/O
    // errors (EBADF, EIO, fcntl-unsupported FS) are NOT contention;
    // surface them so the operator sees that locking is broken on
    // this host instead of a permanent silent no-op.
    match lock_file.try_lock_exclusive() {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
        Err(err) => {
            return Err(anyhow::Error::new(err).context(format!("locking {}", lock_path.display())));
        }
    }

    let want_running = count_keep_awake_agents(runner)? > 0;
    let pid_path = paths.data_dir.join(PID_FILENAME);
    let current_pid = read_pid_file(&pid_path)?;
    let liveness = current_pid.map_or(Liveness::Gone, is_caffeinate_alive_at);

    match (want_running, liveness) {
        (true, Liveness::Alive) => {}
        (true, Liveness::Gone) => {
            // `write_pid_file` truncates+overwrites, so no need to
            // pre-clean the stale PID file. CRITICAL: capture the
            // freshly-spawned PID *before* attempting the write so that
            // if the write fails we can SIGTERM the orphan before
            // propagating — otherwise the detached caffeinate would
            // run until reboot with no recoverable handle (we'd lose
            // the PID with the stack frame).
            let pid = spawn_caffeinate()?;
            if let Err(err) = write_pid_file(&pid_path, pid) {
                stop_caffeinate(pid);
                return Err(err);
            }
        }
        (false, Liveness::Alive) => {
            if let Some(pid) = current_pid {
                stop_caffeinate(pid);
            }
            remove_pid_file_if_present(&pid_path)?;
        }
        (false, Liveness::Gone) => {
            // Process is gone (or PID was reassigned) but the PID file
            // lingered — clean up so future reconciliations don't keep
            // parsing dead state.
            remove_pid_file_if_present(&pid_path)?;
        }
        (_, Liveness::Unknown) => {
            // `ps` couldn't tell us whether caffeinate is alive (binary
            // missing, EAGAIN under fork pressure, weird stdout). Don't
            // act on a guess: leaving the PID file in place lets a
            // future reconcile retry once the environment recovers.
            // Acting blind would either orphan a live caffeinate
            // (false → remove PID file) or spawn a duplicate
            // (true → spawn over an unrecorded survivor).
            eprintln!(
                "[jackin] keep_awake: ps liveness check inconclusive for recorded PID {} — leaving caffeinate state untouched, will retry on next reconcile",
                current_pid.expect("Liveness::Unknown implies a recorded PID")
            );
        }
    }

    Ok(())
}

/// Remove the PID file if it exists, surfacing every error except
/// "already gone." `let _ = remove_file(...)` would also swallow
/// EACCES / EROFS, which are the cases an operator most needs to
/// know about (jackin can no longer manage its own state).
fn remove_pid_file_if_present(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(anyhow::Error::new(err).context(format!("removing {}", path.display()))),
    }
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
    // `trim().is_empty()` (vs `is_empty()`) is defensive against stray
    // whitespace lines — a `\r` or space-prefixed entry would
    // otherwise inflate the count and pin caffeinate when no agents
    // are actually running.
    Ok(output.lines().filter(|l| !l.trim().is_empty()).count())
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

/// What we know about the recorded PID after consulting `ps`.
///
/// The third state matters: collapsing "process gone" and "ps couldn't
/// tell us" into a single `false` would let a transient `ps` failure
/// orphan a live caffeinate by deleting our only handle to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Liveness {
    /// PID exists and is `caffeinate` — keep the assertion.
    Alive,
    /// PID exists but isn't `caffeinate` (reused) **or** doesn't exist
    /// at all. In both cases the recorded PID is no longer ours and
    /// the PID file should be cleared.
    Gone,
    /// We could not determine liveness — `ps` couldn't run, returned
    /// non-UTF8, etc. The reconciler should leave state untouched and
    /// retry on the next call.
    Unknown,
}

/// Run `ps -p PID -o comm=` and classify the result.
///
/// macOS PIDs cycle through ~99k values and are reused quickly. After
/// jackin exits, the OS may reassign our recorded PID to an unrelated
/// user-owned process. A bare `kill -0 PID` would treat that as
/// "still ours" and a later reconcile could SIGTERM the unrelated
/// process. Checking the process basename against `caffeinate` closes
/// that race — at the cost of one extra `ps` exec per reconcile.
fn is_caffeinate_alive_at(pid: u32) -> Liveness {
    // One immediate retry handles EAGAIN-style fork pressure where
    // the first `ps` exec fails but a second one immediately
    // succeeds. Permanent failures (binary missing, PATH broken)
    // fall through to `Unknown` so the reconciler can leave state
    // alone rather than guessing.
    for _ in 0..2 {
        if let Ok(output) = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .stderr(Stdio::null())
            .output()
        {
            return classify_ps_comm_output(
                output.status.success(),
                &String::from_utf8_lossy(&output.stdout),
            );
        }
    }
    Liveness::Unknown
}

/// Pure classification of `ps -p PID -o comm=` output. Split out so
/// the parsing rules (basename normalization across mac/linux, comm
/// match) are unit-testable without spawning real processes.
///
/// On macOS `ps -o comm=` reports the absolute path (e.g.
/// `/usr/bin/caffeinate`); on Linux it reports the basename
/// (potentially truncated to 15 chars, but `caffeinate` is 10).
/// Splitting on `/` and taking the last component normalizes both.
fn classify_ps_comm_output(success: bool, stdout: &str) -> Liveness {
    if !success {
        // `ps -p` exits nonzero only when no PID matches — unambiguous.
        return Liveness::Gone;
    }
    let basename = stdout.trim().rsplit('/').next().unwrap_or("");
    if basename == "caffeinate" {
        Liveness::Alive
    } else {
        // PID was reused by an unrelated process. Treat as gone for
        // PID-file purposes; the caller must never SIGTERM the
        // impostor.
        Liveness::Gone
    }
}

/// Spawn `caffeinate -imsu` so it survives jackin exiting *and* the
/// controlling terminal closing. We can't call `setsid(2)` directly
/// without `unsafe` (forbidden crate-wide), so we shell out via
/// `nohup`, which sets `SIG_IGN` on `SIGHUP` for the child. The
/// wrapper shell exits immediately after backgrounding caffeinate,
/// which is then reparented to launchd — that orphan-reparenting is
/// what actually frees caffeinate from the terminal, not nohup
/// itself (POSIX `nohup` only ignores SIGHUP; it does not call
/// `setsid`).
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

    let raw = String::from_utf8(output.stdout).context("caffeinate PID output not UTF-8")?;
    let pid: u32 = raw
        .trim()
        .parse()
        .with_context(|| format!("parsing caffeinate PID from {:?}", raw.trim()))?;
    Ok(pid)
}

/// SIGTERM the caffeinate process. Non-success results are surfaced
/// via stderr rather than dropped: ESRCH (process exited between our
/// comm check and the kill) is harmless, but EPERM means the PID
/// flipped to a process owned by someone else — the very TOCTOU the
/// comm check exists to prevent — and the operator should at least
/// see a breadcrumb if it ever fires.
fn stop_caffeinate(pid: u32) {
    let result = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    match result {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let trimmed = stderr.trim();
            if trimmed.is_empty() {
                eprintln!("[jackin] keep_awake: kill {pid} exited {}", out.status);
            } else {
                eprintln!("[jackin] keep_awake: kill {pid}: {trimmed}");
            }
        }
        Err(err) => {
            eprintln!("[jackin] keep_awake: failed to spawn kill({pid}): {err}");
        }
    }
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
    fn is_caffeinate_alive_at_returns_gone_for_nonexistent_pid() {
        // PID 1 always exists; pick a deliberately huge number unlikely
        // to be allocated. `ps -p` returns nonzero for missing PIDs.
        assert_eq!(is_caffeinate_alive_at(2_000_000_000), Liveness::Gone);
    }

    #[test]
    fn is_caffeinate_alive_at_returns_gone_for_unrelated_process() {
        // PID 1 is launchd on macOS / init on Linux — alive, but its
        // comm is not "caffeinate". This is exactly the PID-reuse race
        // the comm check guards against; the impostor must classify as
        // `Gone`, not `Alive`, so the caller never SIGTERMs it.
        assert_eq!(is_caffeinate_alive_at(1), Liveness::Gone);
    }

    #[test]
    fn classify_ps_comm_output_returns_gone_on_nonzero_exit() {
        // `ps -p <missing>` exits nonzero with empty stdout — that's
        // the "no such process" signal.
        assert_eq!(classify_ps_comm_output(false, ""), Liveness::Gone);
    }

    #[test]
    fn classify_ps_comm_output_returns_alive_for_basename() {
        // Linux-style: `ps -o comm=` reports just the basename.
        assert_eq!(
            classify_ps_comm_output(true, "caffeinate\n"),
            Liveness::Alive
        );
    }

    #[test]
    fn classify_ps_comm_output_returns_alive_for_absolute_path() {
        // macOS-style: `ps -o comm=` reports the full executable path.
        assert_eq!(
            classify_ps_comm_output(true, "/usr/bin/caffeinate\n"),
            Liveness::Alive,
        );
    }

    #[test]
    fn classify_ps_comm_output_returns_gone_for_other_process() {
        // PID alive but comm doesn't match — same outcome as "no such
        // PID": treat as gone, never act on it.
        assert_eq!(
            classify_ps_comm_output(true, "/sbin/launchd\n"),
            Liveness::Gone
        );
        assert_eq!(classify_ps_comm_output(true, "bash\n"), Liveness::Gone);
    }

    #[test]
    fn classify_ps_comm_output_does_not_match_substring() {
        // Guard against a future "simplification" to `contains` that
        // would treat `caffeinated`, `xcaffeinate`, etc. as a match.
        assert_eq!(
            classify_ps_comm_output(true, "caffeinated\n"),
            Liveness::Gone
        );
        assert_eq!(
            classify_ps_comm_output(true, "xcaffeinate\n"),
            Liveness::Gone
        );
    }

    #[test]
    fn classify_ps_comm_output_returns_gone_for_empty_stdout() {
        // Defensive: success + empty output shouldn't be treated as a
        // match (basename == "" != "caffeinate").
        assert_eq!(classify_ps_comm_output(true, ""), Liveness::Gone);
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
        // Use a PID that is definitely not caffeinate. A huge nonexistent
        // PID exercises the "process gone" branch; PID 1 (launchd/init)
        // would exercise the "alive but wrong comm" branch — both must
        // be treated as "needs cleanup."
        std::fs::write(&pid_path, "2000000001").unwrap();

        let mut runner = FakeRunner::with_capture_queue([String::new()]);
        reconcile_inner(&paths, &mut runner).unwrap();

        assert!(!pid_path.exists(), "stale PID file should be removed");
    }

    #[test]
    fn reconcile_inner_clears_pid_file_when_pid_belongs_to_unrelated_process() {
        let tmp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let pid_path = pid_path_for_tests(&paths);
        // PID 1 is alive on every Unix host, but its comm is launchd /
        // init / systemd, never "caffeinate" — so the comm check should
        // reject it and reconcile should treat the PID file as stale.
        std::fs::write(&pid_path, "1").unwrap();

        let mut runner = FakeRunner::with_capture_queue([String::new()]);
        reconcile_inner(&paths, &mut runner).unwrap();

        assert!(
            !pid_path.exists(),
            "PID file pointing at an unrelated live process should be removed"
        );
    }
}
