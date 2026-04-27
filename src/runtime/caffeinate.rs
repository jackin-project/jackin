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

use super::naming::{FILTER_KEEP_AWAKE, FILTER_MANAGED};

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
            let pid = spawn_caffeinate(runner)?;
            if let Err(err) = write_pid_file(&pid_path, pid) {
                if let Err(stop_err) = stop_caffeinate(runner, pid) {
                    eprintln!(
                        "[jackin] keep_awake: PID file write failed AND cleanup kill of newly-spawned caffeinate (PID {pid}) also failed: {stop_err}; manual `pkill caffeinate` may be required"
                    );
                }
                return Err(err);
            }
        }
        (false, Liveness::Alive) => {
            if let Some(pid) = current_pid {
                // Surface kill failure rather than swallowing it:
                // removing the PID file after a failed kill would
                // orphan caffeinate (next reconcile reads no PID →
                // `Gone` → no-op, and any later `(true, Gone)` arm
                // would spawn a duplicate next to the orphan).
                // Propagating with `?` keeps the PID file intact so
                // the next reconcile retries the same PID.
                stop_caffeinate(runner, pid)?;
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
///
/// The `FILTER_MANAGED` co-filter scopes the count to containers
/// owned by a jackin install (multiple `--filter` flags AND together
/// in `docker ps`). Without it, a container labelled
/// `jackin.keep_awake=true` from a stale or external source — for
/// example, an old jackin install whose state was uninstalled but
/// whose containers were left running — would pin our caffeinate
/// indefinitely with no way to discover why.
fn count_keep_awake_agents(runner: &mut impl CommandRunner) -> anyhow::Result<usize> {
    let output = runner.capture(
        "docker",
        &[
            "ps",
            "--filter",
            FILTER_MANAGED,
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
        Ok(contents) => {
            let trimmed = contents.trim();
            // Distinguish "file empty" from "file has unparseable bytes."
            // Empty → behave like "no PID recorded" (treat as fresh
            // start). Unparseable → propagate so the outer reconcile()
            // breadcrumb fires; silently coercing to None would let a
            // corrupted PID file orphan a live caffeinate by spawning a
            // duplicate over the unrecorded survivor.
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed.parse::<u32>().map(Some).map_err(|e| {
                anyhow::Error::new(e).context(format!(
                    "PID file {} contains non-numeric data; refusing to overwrite to avoid orphaning caffeinate",
                    path.display()
                ))
            })
        }
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
/// without `unsafe` (forbidden crate-wide), and `setsid(1)` is not
/// installed on stock macOS, so we shell out via `nohup`, which sets
/// `SIG_IGN` on `SIGHUP` for the child. The wrapper shell exits
/// immediately after backgrounding caffeinate, which is then
/// reparented to launchd.
///
/// ## Caveat: process group is not escaped
///
/// `nohup` only ignores SIGHUP; it does not start a new session, and
/// neither does the wrapper shell. The detached caffeinate inherits
/// jackin's process group ID. Two practical consequences:
///
/// 1. Closing the controlling terminal is safe — SIGHUP is ignored
///    by the child and the terminal-driven SIGHUP would land on a
///    process group whose only foreground member (jackin) has
///    already exited.
/// 2. A *group-targeted* signal (e.g. `kill -TERM -<pgid>`, or some
///    process-supervisor tooling) sent to jackin's original PGID
///    after jackin has exited will also reach the orphaned
///    caffeinate. In typical interactive shell use this never fires
///    (Ctrl-C targets the foreground PGID, and there is no
///    foreground process in the original PGID once jackin exits),
///    but we cannot guarantee group isolation without `unsafe`.
fn spawn_caffeinate(runner: &mut impl CommandRunner) -> anyhow::Result<u32> {
    // Routed through `CommandRunner` so `--debug` surfaces the spawn
    // (`[debug] sh -c …`) and the resulting PID (`[debug] -> <pid>`).
    // Operators validating keep_awake need to see this transition or
    // the reconciler is opaque from the outside.
    let raw = runner.capture(
        "sh",
        &[
            "-c",
            "nohup caffeinate -imsu </dev/null >/dev/null 2>&1 & echo $!",
        ],
        None,
    )?;
    raw.parse::<u32>()
        .with_context(|| format!("parsing caffeinate PID from {raw:?}"))
}

/// SIGTERM the caffeinate process.
///
/// Returns `Err` on any failure (kill exit nonzero, kill itself fails
/// to spawn). The error carries the PID and the kill stderr so the
/// outer `reconcile()` breadcrumb is actionable. Callers that hold a
/// PID file MUST decide what to do with it on failure: removing the
/// PID file when the kill failed orphans caffeinate (we lose the
/// only handle to the live process), so the `(false, Alive)` arm
/// propagates the error and leaves the PID file intact for the next
/// reconcile to retry against the same PID.
///
/// ESRCH (process exited between our comm check and the kill) and
/// EPERM (PID flipped to a process owned by someone else — the very
/// TOCTOU the comm check exists to prevent) both surface here so the
/// operator sees a breadcrumb when the rare race fires.
fn stop_caffeinate(runner: &mut impl CommandRunner, pid: u32) -> anyhow::Result<()> {
    // Routed through `CommandRunner` for the same reason as the spawn:
    // `--debug` must show the kill so operators can correlate the
    // teardown with the agent exit. `capture` (vs `run`) folds the
    // kill's stderr into the error message — preserving the prior
    // behaviour where `ESRCH`/`EPERM` text reached the breadcrumb.
    runner
        .capture("kill", &[&pid.to_string()], None)
        .map(|_| ())
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
            "docker ps --filter label=jackin.managed=true --filter label=jackin.keep_awake=true --format {{.Names}}"
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
    fn read_pid_file_returns_none_for_empty_file() {
        // Empty file is the legitimate "no PID recorded" state — treat
        // as a fresh start, not as corruption.
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.pid");
        std::fs::write(&path, "").unwrap();
        assert_eq!(read_pid_file(&path).unwrap(), None);
    }

    #[test]
    fn read_pid_file_errors_on_garbage() {
        // Corrupted PID file (non-numeric content) MUST surface as an
        // error rather than coercing to None — silent coercion would
        // let the next `(true, Gone)` arm spawn a duplicate caffeinate
        // over the unrecorded survivor, orphaning the prior process
        // until reboot.
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.pid");
        std::fs::write(&path, "not-a-pid").unwrap();
        let err = read_pid_file(&path).unwrap_err();
        assert!(
            err.to_string().contains("non-numeric"),
            "error must mention non-numeric content; got: {err}",
        );
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
