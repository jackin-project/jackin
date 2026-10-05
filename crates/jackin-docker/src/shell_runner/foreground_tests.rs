// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

//! PTY regressions for interactive process-group ownership.
//!
//! These tests run the async ShellRunner inside a fresh session created by the
//! Python stdlib harness. The test process therefore owns a controlling PTY,
//! while the outer Rust test remains detached from the developer's terminal.

use super::*;
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

const FIXTURE_ENV: &str = "JACKIN_INTERACTIVE_FOREGROUND_FIXTURE";

// Keep this harness stdlib-only. pty.openpty plus an explicit setsid/TIOCSCTTY
// makes the Rust test binary a session leader with a private controlling PTY.
const PTY_HARNESS: &str = r#"
import errno
import fcntl
import os
import pty
import select
import signal
import sys
import termios
import time

if sys.platform.startswith("linux"):
    import ctypes
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:
        error = ctypes.get_errno()
        raise OSError(error, "prctl(PR_SET_CHILD_SUBREAPER) failed")

def reap_adopted_children(excluded_pid):
    # Linux subreaper mode adopts the shell's descendants. Reap only a child
    # selected by waitid(WNOWAIT), never waitpid(-1), so the original helper's
    # status remains reserved for the explicit waitpid(child) below.
    if (not sys.platform.startswith("linux") or not hasattr(os, "waitid")
            or not hasattr(os, "WNOWAIT")):
        return
    options = os.WEXITED | os.WNOHANG | os.WNOWAIT
    while True:
        try:
            observed = os.waitid(os.P_ALL, 0, options)
        except (ChildProcessError, OSError):
            return
        if observed is None or observed.si_pid == 0:
            return
        if observed.si_pid == excluded_pid:
            return
        try:
            os.waitpid(observed.si_pid, 0)
        except ChildProcessError:
            pass

binary = os.fsdecode(sys.argv[1])
mode = sys.argv[2]
environment = os.environ.copy()
environment["JACKIN_INTERACTIVE_FOREGROUND_FIXTURE"] = mode
master, slave = pty.openpty()
child = os.fork()
if child == 0:
    try:
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        os.dup2(slave, 0)
        os.dup2(slave, 1)
        os.dup2(slave, 2)
        os.close(master)
        os.close(slave)
        if mode == "background":
            background = os.fork()
            if background == 0:
                os.setpgid(0, 0)
                os.execve(binary, [binary, "--exact", "shell_runner::foreground_tests::interactive_fixture_entrypoint", "--nocapture"], environment)
            _, child_status = os.waitpid(background, 0)
            os._exit(os.waitstatus_to_exitcode(child_status))
        os.execve(binary, [binary, "--exact", "shell_runner::foreground_tests::interactive_fixture_entrypoint", "--nocapture"], environment)
    except BaseException as error:
        message = ("PTY fixture exec failed: " + repr(error) + "\n").encode()
        try:
            os.write(2, message)
        finally:
            os._exit(127)

os.close(slave)
os.set_blocking(master, False)
output = bytearray()
input_sent = False
status = None
deadline = time.monotonic() + 15.0
while status is None:
    reap_adopted_children(child)
    if time.monotonic() >= deadline:
        # The helper is still our unreaped child, so its own process group is
        # the only safe signal target. Runtime-owned descendants keep their
        # normal cleanup path; never scan and signal recycled numeric groups.
        try:
            os.killpg(child, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
        _, status = os.waitpid(child, 0)
        reap_adopted_children(child)
        output.extend(b"PTY fixture harness deadline expired\n")
        break
    readable, _, _ = select.select([master], [], [], 0.05)
    if readable:
        try:
            output.extend(os.read(master, 65536))
            if not input_sent and b"CHILD_PID=" in output:
                os.write(master, b"j\n")
                input_sent = True
        except OSError as error:
            if error.errno not in (errno.EIO, errno.EBADF):
                raise
    waited, candidate = os.waitpid(child, os.WNOHANG)
    if waited == child:
        status = candidate

if status is not None:
    reap_adopted_children(child)

# Drain the PTY after the helper exits so assertion text and terminal probes
# reach the outer Rust test. A leaked slave holder must not hold this loop.
for _ in range(5):
    readable, _, _ = select.select([master], [], [], 0.02)
    if not readable:
        break
    try:
        output.extend(os.read(master, 65536))
    except OSError as error:
        if error.errno not in (errno.EIO, errno.EBADF):
            raise
os.close(master)
sys.stdout.buffer.write(output)
sys.stdout.flush()
sys.exit(os.waitstatus_to_exitcode(status))
"#;

const FOREGROUND_PROBE: &str = r#"
import os,signal,termios
attrs=termios.tcgetattr(0)
attrs[3] &= ~(termios.ICANON|termios.ECHO)
termios.tcsetattr(0, termios.TCSANOW, attrs)
mask=signal.pthread_sigmask(signal.SIG_BLOCK, [])
print("CHILD_PID=%d CHILD_PGID=%d FOREGROUND_PGID=%d SIGTTOU_BLOCKED=%s SIGNAL_MASK=%s" %
      (os.getpid(), os.getpgrp(), os.tcgetpgrp(0), str(signal.SIGTTOU in mask).lower(),
       sorted(int(number) for number in mask)), flush=True)
data=os.read(0, 1)
print("READ_OK=true" if data == b"j" else "READ_OK=false", flush=True)
"#;

// The shell and sleep are deliberately separate processes in the same private
// process group. The Python child changes terminal modes before it blocks so a
// forced group cleanup must restore both foreground ownership and termios.
const CONTROLLED_FIXTURE: &str = r#"
printf '%s\n' "$$" > "$1"
sleep 30 &
printf '%s\n' "$!" >> "$1"
python3 -c 'import pathlib,sys,termios,time; attrs=termios.tcgetattr(0); attrs[3] &= ~(termios.ICANON|termios.ECHO); termios.tcsetattr(0,termios.TCSANOW,attrs); pathlib.Path(sys.argv[1]).write_text("ready"); print("READY", flush=True); time.sleep(30)' "$2"
"#;

// This helper is an independently registered child, outside the cancelled
// ShellRunner group. It owns the PTY only until the Rust fixture authorizes
// release, then explicitly returns foreground ownership to the caller group.
const FOREIGN_FOREGROUND_HELPER: &str = r#"
import os,pathlib,signal,sys,time
original_pgid=int(sys.argv[1])
ready_path=pathlib.Path(sys.argv[2])
release_path=pathlib.Path(sys.argv[3])
os.setpgid(0, 0)
signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTTOU})
foreground_owned=False
try:
    os.tcsetpgrp(0, os.getpgrp())
    foreground_owned=True
    ready_path.write_text("ready")
    deadline=time.monotonic()+10.0
    while not release_path.exists():
        if time.monotonic() >= deadline:
            raise TimeoutError("foreground release file was not provided")
        time.sleep(0.01)
finally:
    if foreground_owned:
        os.tcsetpgrp(0, original_pgid)
"#;

const CAPTURE_CONTRADICTION_FIXTURE: &str =
    "printf 'spawned' > \"$1\"; printf 'stdout\\n'; printf 'stderr\\n' >&2";

#[derive(Debug, Eq, PartialEq)]
struct TerminalSnapshot {
    foreground_pgid: i64,
    attributes: String,
    signal_mask: String,
}

fn run_pty_fixture(mode: &str) -> String {
    let binary = std::env::current_exe().expect("test binary path");
    let request = jackin_process::ExecRequest::new(
        "python3",
        [
            OsString::from("-c"),
            OsString::from(PTY_HARNESS),
            binary.into_os_string(),
            OsString::from(mode),
        ],
    )
    .stdin_mode(jackin_process::StdioMode::Null)
    .stdout_mode(jackin_process::StdioMode::Capture)
    .stderr_mode(jackin_process::StdioMode::Capture);
    let child = jackin_process::spawn_sync(&request).expect("spawn PTY harness");
    let output = child.wait_with_output().expect("reap PTY harness child");
    let mut text = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "PTY fixture failed with {:?}:\n{text}",
        output.status
    );
    text
}

fn run_direct_fixture(mode: &str) -> String {
    let binary = std::env::current_exe().expect("test binary path");
    let request = jackin_process::ExecRequest::new(
        binary,
        [
            OsString::from("--exact"),
            OsString::from("shell_runner::foreground_tests::interactive_fixture_entrypoint"),
            OsString::from("--nocapture"),
        ],
    )
    .envs([(FIXTURE_ENV, mode)])
    .stdin_mode(jackin_process::StdioMode::Null)
    .stdout_mode(jackin_process::StdioMode::Capture)
    .stderr_mode(jackin_process::StdioMode::Capture);
    let child = jackin_process::spawn_sync(&request).expect("spawn direct fixture");
    let output = child.wait_with_output().expect("reap direct fixture child");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "direct fixture failed with {:?}:\n{text}",
        output.status
    );
    text
}

fn terminal_snapshot() -> TerminalSnapshot {
    // Darwin defers canonical replay after native raw-to-canonical restoration.
    // Observing available input settles PENDIN without reading or flushing it.
    // Compare the complete termios snapshot after that normal driver operation;
    // no configuration bits are masked from the equality check.
    let request = jackin_process::ExecRequest::new(
        "python3",
        [
            "-c",
            "import array,fcntl,os,signal,termios; fcntl.ioctl(0, termios.FIONREAD, array.array('i', [0]), True); print(os.tcgetpgrp(0)); print(repr(termios.tcgetattr(0))); print(repr(sorted(int(number) for number in signal.pthread_sigmask(signal.SIG_BLOCK, []))))",
        ],
    )
    .stdin_mode(jackin_process::StdioMode::Inherit)
    .stdout_mode(jackin_process::StdioMode::Capture)
    .stderr_mode(jackin_process::StdioMode::Capture);
    let child = jackin_process::spawn_sync(&request).expect("spawn terminal probe");
    let output = child.wait_with_output().expect("reap terminal probe");
    assert!(
        output.status.success(),
        "terminal probe failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut lines = stdout.lines();
    let foreground_pgid: i64 = lines
        .next()
        .expect("terminal probe foreground group")
        .trim()
        .parse()
        .expect("numeric foreground group");
    let attributes = lines.next().expect("terminal probe attributes").to_owned();
    assert!(!attributes.is_empty(), "terminal probe attributes");
    let signal_mask = lines.next().expect("terminal probe signal mask").to_owned();
    assert!(
        lines.next().is_none(),
        "terminal probe emitted unexpected output: {stdout:?}"
    );
    TerminalSnapshot {
        foreground_pgid,
        attributes,
        signal_mask,
    }
}

fn interactive_options(timeout: Option<Duration>) -> RunOptions {
    RunOptions {
        interactive: true,
        timeout,
        ..RunOptions::default()
    }
}

fn marker<'a>(output: &'a str, prefix: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("missing {prefix:?} in PTY output:\n{output}"))
}

fn assert_child_foreground_probe(output: &str) {
    let probe = output
        .lines()
        .find(|line| line.starts_with("CHILD_PID="))
        .unwrap_or_else(|| panic!("missing child foreground probe:\n{output}"));
    let mut fields = probe.split_whitespace();
    let child_pgid = fields
        .find_map(|field| field.strip_prefix("CHILD_PGID=")?.parse::<i64>().ok())
        .expect("child process group probe");
    let foreground_pgid = probe
        .split_whitespace()
        .find_map(|field| field.strip_prefix("FOREGROUND_PGID=")?.parse::<i64>().ok())
        .expect("child foreground process group probe");
    assert_eq!(
        foreground_pgid, child_pgid,
        "interactive child did not own the terminal immediately:\n{output}"
    );
    let sigttou_blocked = probe
        .split_whitespace()
        .find_map(|field| field.strip_prefix("SIGTTOU_BLOCKED="))
        .expect("child SIGTTOU mask probe");
    assert_eq!(
        sigttou_blocked, "false",
        "interactive child unexpectedly blocked SIGTTOU:\n{output}"
    );
}

fn fixture_pids(path: &Path) -> (i32, i32) {
    let text = std::fs::read_to_string(path).expect("controlled fixture PID file");
    let mut lines = text.lines();
    let leader = lines
        .next()
        .expect("controlled fixture leader PID")
        .parse()
        .expect("numeric controlled fixture leader PID");
    let descendant = lines
        .next()
        .expect("controlled fixture descendant PID")
        .parse()
        .expect("numeric controlled fixture descendant PID");
    (leader, descendant)
}

fn fixture_process_live(pid: i32) -> bool {
    let pid_text = pid.to_string();
    let request = jackin_process::ExecRequest::new("ps", ["-o", "stat=", "-p", pid_text.as_str()])
        .stdin_mode(jackin_process::StdioMode::Null)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Capture);
    let output = jackin_process::exec_sync(&request).unwrap_or_else(|error| {
        panic!("ps failed while checking fixture process {pid}: {error:#}")
    });
    assert!(
        !output.timed_out,
        "ps timed out while checking fixture process {pid}"
    );
    if !output.success {
        assert_eq!(
            output.code,
            Some(1),
            "ps returned unexpected status while checking fixture process {pid}: stdout={:?}, stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stdout.is_empty(),
            "ps reported a failed fixture lookup with stdout for {pid}: {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            output.stderr.is_empty(),
            "ps reported an unexpected fixture lookup error for {pid}: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        return false;
    }
    assert_eq!(
        output.code,
        Some(0),
        "ps reported success without exit code zero for fixture process {pid}"
    );
    assert!(
        output.stderr.is_empty(),
        "ps emitted an unexpected fixture lookup warning for {pid}: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let state = String::from_utf8_lossy(&output.stdout)
        .trim_start()
        .chars()
        .next()
        .unwrap_or_else(|| panic!("ps returned no state for live fixture process {pid}"));
    state != 'Z'
}

async fn wait_fixture_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "fixture did not signal readiness: {path:?}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn wait_fixture_process_gone(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while fixture_process_live(pid) {
        assert!(
            Instant::now() < deadline,
            "fixture process {pid} survived group cleanup"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_terminal_ownership_released() {
    let deadline = Instant::now() + Duration::from_secs(2);
    while jackin_diagnostics::rich_terminal_owned() {
        assert!(
            Instant::now() < deadline,
            "logical terminal ownership remained after foreground cleanup"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!jackin_diagnostics::rich_terminal_owned());
}

async fn wait_foreground_cleanup_error() {
    let deadline = Instant::now() + Duration::from_secs(2);
    while jackin_process_directory::foreground_cleanup_error().is_none() {
        assert!(
            Instant::now() < deadline,
            "foreground cleanup failure was not retained"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(jackin_process_directory::foreground_cleanup_error().is_some());
}

async fn wait_terminal_restored(expected: &TerminalSnapshot) -> TerminalSnapshot {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let current = terminal_snapshot();
        if &current == expected {
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "foreground terminal state was not restored: expected {expected:?}, got {current:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn interactive_normal_exit_restores_foreground_and_termios() {
    let output = run_pty_fixture("normal");
    assert!(output.contains("INTERACTIVE_FIXTURE=normal"), "{output}");
    let before_pgid: i64 = marker(&output, "BEFORE_PGID=")
        .parse()
        .expect("before PGID");
    let after_pgid: i64 = marker(&output, "AFTER_PGID=").parse().expect("after PGID");
    assert_eq!(
        before_pgid, after_pgid,
        "normal exit changed caller foreground group"
    );
    assert_eq!(marker(&output, "ATTRIBUTES_RESTORED="), "true");
    assert_eq!(marker(&output, "SIGNAL_MASK_RESTORED="), "true");
    assert_eq!(marker(&output, "READ_OK="), "true");
    assert_eq!(marker(&output, "NORMAL_GROUP_GONE="), "true");
    assert_child_foreground_probe(&output);
}

#[test]
fn interactive_spawn_failure_restores_foreground_and_termios() {
    let output = run_pty_fixture("spawn-failure");
    assert!(
        output.contains("INTERACTIVE_FIXTURE=spawn-failure"),
        "{output}"
    );
    assert_eq!(marker(&output, "SPAWN_FAILED="), "true");
    assert_eq!(
        marker(&output, "BEFORE_PGID="),
        marker(&output, "AFTER_PGID=")
    );
    assert_eq!(marker(&output, "ATTRIBUTES_RESTORED="), "true");
    assert_eq!(marker(&output, "SIGNAL_MASK_RESTORED="), "true");
}

#[test]
fn interactive_background_caller_is_rejected_without_stealing_foreground() {
    let output = run_pty_fixture("background");
    assert!(
        output.contains("INTERACTIVE_FIXTURE=background"),
        "{output}"
    );
    assert_eq!(marker(&output, "BACKGROUND_REJECTED="), "true");
    assert_eq!(
        marker(&output, "BEFORE_PGID="),
        marker(&output, "AFTER_PGID=")
    );
    assert_eq!(marker(&output, "ATTRIBUTES_RESTORED="), "true");
    assert_eq!(marker(&output, "SIGNAL_MASK_RESTORED="), "true");
}

#[test]
fn interactive_non_tty_keeps_private_group_timeout_cleanup() {
    let output = run_direct_fixture("non-tty");
    assert!(output.contains("INTERACTIVE_FIXTURE=non-tty"), "{output}");
    assert_eq!(marker(&output, "NONTTY_STDIN_ISATTY="), "false");
    assert_eq!(marker(&output, "NONTTY_TIMEOUT="), "true");
}

#[test]
fn interactive_capture_options_reject_before_spawn() {
    let output = run_direct_fixture("invalid-options");
    assert!(
        output.contains("INTERACTIVE_FIXTURE=invalid-options"),
        "{output}"
    );
    assert_eq!(marker(&output, "INVALID_OPTIONS_TYPED="), "true");
    assert_eq!(marker(&output, "INVALID_OPTIONS_UNSPAWNED="), "true");
    assert_eq!(
        marker(&output, "INVALID_OPTIONS_NO_TERMINAL_CLAIM="),
        "true"
    );
    assert_eq!(marker(&output, "NONINTERACTIVE_CONTROL="), "true");
}

#[test]
fn interactive_timeout_kills_shell_descendant_group_and_restores_terminal() {
    let output = run_pty_fixture("timeout");
    assert!(output.contains("INTERACTIVE_FIXTURE=timeout"), "{output}");
    assert_eq!(marker(&output, "TIMEOUT="), "true");
    assert_eq!(marker(&output, "ATTRIBUTES_RESTORED="), "true");
    assert_eq!(marker(&output, "SIGNAL_MASK_RESTORED="), "true");
    assert_eq!(marker(&output, "GROUP_GONE="), "true");
}

#[test]
fn interactive_cancel_kills_shell_descendant_group_and_restores_terminal() {
    let output = run_pty_fixture("cancel");
    assert!(output.contains("INTERACTIVE_FIXTURE=cancel"), "{output}");
    assert_eq!(marker(&output, "CANCELLED="), "true");
    assert_eq!(marker(&output, "ATTRIBUTES_RESTORED="), "true");
    assert_eq!(marker(&output, "SIGNAL_MASK_RESTORED="), "true");
    assert_eq!(marker(&output, "GROUP_GONE="), "true");
}

#[test]
fn interactive_restore_failure_retains_scope_until_foreground_returns() {
    let output = run_pty_fixture("restore-failure");
    assert!(
        output.contains("INTERACTIVE_FIXTURE=restore-failure"),
        "{output}"
    );
    assert_eq!(marker(&output, "RESTORE_FAILURE="), "true");
    assert_eq!(
        marker(&output, "FOREGROUND_CLEANUP_ERROR_RETAINED="),
        "true"
    );
    assert_eq!(marker(&output, "RESTORE_ERROR_PERMISSION_DENIED="), "true");
    assert_eq!(
        marker(&output, "OWNERSHIP_RETAINED_DURING_FAILURE="),
        "true"
    );
    assert_eq!(marker(&output, "FOREGROUND_CLEANUP_ERROR_CLEARED="), "true");
    assert_eq!(marker(&output, "EXTERNAL_SCOPE_RELEASED="), "true");
    assert_eq!(marker(&output, "ATTRIBUTES_RESTORED="), "true");
    assert_eq!(marker(&output, "SIGNAL_MASK_RESTORED="), "true");
    assert_eq!(marker(&output, "RESTORE_RELEASED="), "true");
}

#[tokio::test(flavor = "current_thread")]
async fn interactive_fixture_entrypoint() {
    let Some(mode) = std::env::var_os(FIXTURE_ENV) else {
        return;
    };
    let mode = mode.to_string_lossy();
    println!("INTERACTIVE_FIXTURE={mode}");
    match mode.as_ref() {
        "normal" => {
            let temporary = tempfile::tempdir().expect("normal fixture tempdir");
            let pids_path = temporary.path().join("pids");
            let before = terminal_snapshot();
            let script = format!(
                "sleep 30 &\nprintf '%s\\n%s\\n' \"$$\" \"$!\" > \"$1\"\npython3 -c '{FOREGROUND_PROBE}'\nexit 0"
            );
            let mut runner = ShellRunner::default();
            runner
                .run(
                    "sh",
                    &[
                        "-c",
                        &script,
                        "fixture",
                        pids_path.to_str().expect("normal PID path"),
                    ],
                    None,
                    &interactive_options(None),
                )
                .await
                .expect("interactive normal command");
            let (leader, descendant) = fixture_pids(&pids_path);
            wait_fixture_process_gone(leader).await;
            wait_fixture_process_gone(descendant).await;
            wait_terminal_ownership_released().await;
            let after = wait_terminal_restored(&before).await;
            println!("BEFORE_PGID={}", before.foreground_pgid);
            println!("AFTER_PGID={}", after.foreground_pgid);
            println!(
                "ATTRIBUTES_RESTORED={}",
                before.attributes == after.attributes
            );
            println!(
                "SIGNAL_MASK_RESTORED={}",
                before.signal_mask == after.signal_mask
            );
            println!("NORMAL_GROUP_GONE=true");
            assert_eq!(before, after);
        }
        "spawn-failure" => {
            let before = terminal_snapshot();
            let mut runner = ShellRunner::default();
            let result = runner
                .run(
                    "/jackin/definitely/missing/interactive-fixture",
                    &[],
                    None,
                    &interactive_options(None),
                )
                .await;
            assert!(
                result.is_err(),
                "missing interactive executable unexpectedly spawned"
            );
            wait_terminal_ownership_released().await;
            let after = terminal_snapshot();
            println!("SPAWN_FAILED=true");
            println!("BEFORE_PGID={}", before.foreground_pgid);
            println!("AFTER_PGID={}", after.foreground_pgid);
            println!(
                "ATTRIBUTES_RESTORED={}",
                before.attributes == after.attributes
            );
            println!(
                "SIGNAL_MASK_RESTORED={}",
                before.signal_mask == after.signal_mask
            );
            assert_eq!(before, after);
        }
        "background" => {
            let before = terminal_snapshot();
            let mut runner = ShellRunner::default();
            let result = runner
                .run("true", &[], None, &interactive_options(None))
                .await
                .expect_err("background interactive caller must be rejected");
            assert!(
                result
                    .to_string()
                    .contains("caller does not own terminal foreground"),
                "unexpected background rejection: {result:#}"
            );
            wait_terminal_ownership_released().await;
            let after = terminal_snapshot();
            println!("BACKGROUND_REJECTED=true");
            println!("BEFORE_PGID={}", before.foreground_pgid);
            println!("AFTER_PGID={}", after.foreground_pgid);
            println!(
                "ATTRIBUTES_RESTORED={}",
                before.attributes == after.attributes
            );
            println!(
                "SIGNAL_MASK_RESTORED={}",
                before.signal_mask == after.signal_mask
            );
            assert_eq!(before, after);
        }
        "non-tty" => {
            let options = RunOptions {
                interactive: true,
                null_stdin: true,
                timeout: Some(Duration::from_millis(300)),
                ..RunOptions::default()
            };
            let args = [
                "-c",
                "import os,time; print('NONTTY_STDIN_ISATTY=%s' % str(os.isatty(0)).lower(), flush=True); time.sleep(30)",
            ];
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                ShellRunner::default().run("python3", &args, None, &options),
            )
            .await
            .expect("non-TTY interactive timeout bound")
            .expect_err("non-TTY fixture should time out");
            assert!(matches!(
                result.downcast_ref::<DockerError>(),
                Some(DockerError::CommandTimeout { .. })
            ));
            println!("NONTTY_TIMEOUT=true");
        }
        "invalid-options" => {
            let temporary = tempfile::tempdir().expect("invalid options fixture tempdir");
            let mut typed_cases = 0;
            let mut unspawned = true;
            for (name, capture_stdout, capture_stderr) in [
                ("stdout", true, false),
                ("stderr", false, true),
                ("both", true, true),
            ] {
                let marker_path = temporary.path().join(format!("{name}-ran"));
                let marker_arg = marker_path.to_str().expect("invalid options marker path");
                let options = RunOptions {
                    capture_stdout,
                    capture_stderr,
                    interactive: true,
                    ..RunOptions::default()
                };
                assert!(
                    !jackin_diagnostics::rich_terminal_owned(),
                    "terminal ownership was already claimed before {name} invalid-options case"
                );
                let mut runner = ShellRunner::default();
                let error = runner
                    .run(
                        "sh",
                        &["-c", CAPTURE_CONTRADICTION_FIXTURE, "fixture", marker_arg],
                        None,
                        &options,
                    )
                    .await
                    .expect_err("interactive capture contradiction unexpectedly ran");
                let typed = error
                    .downcast_ref::<ProcessBoundaryError>()
                    .is_some_and(|error| matches!(error, ProcessBoundaryError::InvalidOptions));
                assert!(typed, "invalid-options error was not typed: {error:#}");
                typed_cases += 1;
                if marker_path.exists() {
                    unspawned = false;
                }
                assert!(
                    !jackin_diagnostics::rich_terminal_owned(),
                    "interactive capture rejection claimed terminal ownership for {name}"
                );
            }
            let control_path = temporary.path().join("noninteractive-ran");
            let control_arg = control_path
                .to_str()
                .expect("noninteractive control marker path");
            let control_options = RunOptions {
                capture_stdout: true,
                capture_stderr: true,
                stream_captured_output: false,
                ..RunOptions::default()
            };
            ShellRunner::default()
                .run(
                    "sh",
                    &["-c", CAPTURE_CONTRADICTION_FIXTURE, "fixture", control_arg],
                    None,
                    &control_options,
                )
                .await
                .expect("noninteractive capture control command");
            assert_eq!(
                std::fs::read_to_string(&control_path).expect("noninteractive control marker"),
                "spawned"
            );
            assert_eq!(typed_cases, 3);
            assert!(
                unspawned,
                "an interactive capture contradiction spawned a command"
            );
            assert!(!jackin_diagnostics::rich_terminal_owned());
            println!("INVALID_OPTIONS_TYPED=true");
            println!("INVALID_OPTIONS_UNSPAWNED={unspawned}");
            println!(
                "INVALID_OPTIONS_NO_TERMINAL_CLAIM={}",
                !jackin_diagnostics::rich_terminal_owned()
            );
            println!("NONINTERACTIVE_CONTROL=true");
        }
        "timeout" => {
            let temporary = tempfile::tempdir().expect("timeout fixture tempdir");
            let pids_path = temporary.path().join("pids");
            let ready_path = temporary.path().join("ready");
            let before = terminal_snapshot();
            let options = interactive_options(Some(Duration::from_secs(1)));
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                ShellRunner::default().run(
                    "sh",
                    &[
                        "-c",
                        CONTROLLED_FIXTURE,
                        "fixture",
                        pids_path.to_str().expect("timeout PID path"),
                        ready_path.to_str().expect("timeout ready path"),
                    ],
                    None,
                    &options,
                ),
            )
            .await
            .expect("interactive timeout cleanup bound")
            .expect_err("interactive fixture should time out");
            assert!(matches!(
                result.downcast_ref::<DockerError>(),
                Some(DockerError::CommandTimeout { .. })
            ));
            wait_fixture_file(&ready_path).await;
            let (leader, descendant) = fixture_pids(&pids_path);
            wait_fixture_process_gone(leader).await;
            wait_fixture_process_gone(descendant).await;
            wait_terminal_ownership_released().await;
            let after = wait_terminal_restored(&before).await;
            println!("TIMEOUT=true");
            println!(
                "ATTRIBUTES_RESTORED={}",
                before.attributes == after.attributes
            );
            println!(
                "SIGNAL_MASK_RESTORED={}",
                before.signal_mask == after.signal_mask
            );
            println!("GROUP_GONE=true");
            assert_eq!(before, after);
        }
        "cancel" => {
            let temporary = tempfile::tempdir().expect("cancel fixture tempdir");
            let pids_path = temporary.path().join("pids");
            let ready_path = temporary.path().join("ready");
            let pids = pids_path.to_str().expect("cancel PID path").to_owned();
            let ready = ready_path.to_str().expect("cancel ready path").to_owned();
            let before = terminal_snapshot();
            let task = tokio::spawn(async move {
                let options = interactive_options(None);
                ShellRunner::default()
                    .run(
                        "sh",
                        &[
                            "-c",
                            CONTROLLED_FIXTURE,
                            "fixture",
                            pids.as_str(),
                            ready.as_str(),
                        ],
                        None,
                        &options,
                    )
                    .await
            });
            wait_fixture_file(&ready_path).await;
            assert!(
                jackin_diagnostics::rich_terminal_owned(),
                "interactive scope was not held while the child was ready"
            );
            task.abort();
            let joined = tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .expect("interactive cancellation join bound");
            assert!(
                joined
                    .expect_err("cancelled interactive task must abort")
                    .is_cancelled()
            );
            let (leader, descendant) = fixture_pids(&pids_path);
            wait_fixture_process_gone(leader).await;
            wait_fixture_process_gone(descendant).await;
            wait_terminal_ownership_released().await;
            let after = wait_terminal_restored(&before).await;
            println!("CANCELLED=true");
            println!(
                "ATTRIBUTES_RESTORED={}",
                before.attributes == after.attributes
            );
            println!(
                "SIGNAL_MASK_RESTORED={}",
                before.signal_mask == after.signal_mask
            );
            println!("GROUP_GONE=true");
            assert_eq!(before, after);
        }
        "restore-failure" => {
            let temporary = tempfile::tempdir().expect("restore failure fixture tempdir");
            let pids_path = temporary.path().join("pids");
            let ready_path = temporary.path().join("ready");
            let foreign_ready_path = temporary.path().join("foreign-ready");
            let release_path = temporary.path().join("release");
            let pids = pids_path
                .to_str()
                .expect("restore failure PID path")
                .to_owned();
            let ready = ready_path
                .to_str()
                .expect("restore failure ready path")
                .to_owned();
            let before = terminal_snapshot();
            let shell_task = tokio::spawn(async move {
                let options = interactive_options(None);
                ShellRunner::default()
                    .run(
                        "sh",
                        &[
                            "-c",
                            CONTROLLED_FIXTURE,
                            "fixture",
                            pids.as_str(),
                            ready.as_str(),
                        ],
                        None,
                        &options,
                    )
                    .await
            });
            wait_fixture_file(&ready_path).await;

            let foreign_ready = foreign_ready_path
                .to_str()
                .expect("foreign helper ready path")
                .to_owned();
            let release = release_path
                .to_str()
                .expect("foreign helper release path")
                .to_owned();
            let foreign_args = [
                OsString::from("-c"),
                OsString::from(FOREIGN_FOREGROUND_HELPER),
                OsString::from(before.foreground_pgid.to_string()),
                OsString::from(foreign_ready),
                OsString::from(release),
            ];
            let foreign_request = jackin_process::ExecRequest::new("python3", foreign_args)
                .stdin_mode(jackin_process::StdioMode::Inherit)
                .stdout_mode(jackin_process::StdioMode::Capture)
                .stderr_mode(jackin_process::StdioMode::Capture);
            let mut foreign = jackin_process::spawn_async(&foreign_request)
                .expect("spawn registered foreign foreground helper");
            let foreign_pid = foreign.id().expect("foreign helper PID");
            wait_fixture_file(&foreign_ready_path).await;

            shell_task.abort();
            let joined = tokio::time::timeout(Duration::from_secs(2), shell_task)
                .await
                .expect("restore failure cancellation join bound");
            assert!(
                joined
                    .expect_err("restore failure shell task must abort")
                    .is_cancelled()
            );

            wait_foreground_cleanup_error().await;
            let cleanup_error = jackin_process_directory::foreground_cleanup_error();
            let ownership_retained = jackin_diagnostics::rich_terminal_owned();
            let current = terminal_snapshot();
            assert_eq!(
                current.foreground_pgid,
                i64::from(foreign_pid),
                "foreign helper did not retain terminal foreground ownership"
            );
            assert!(
                ownership_retained,
                "terminal scope released after restore failure"
            );
            let permission_denied = cleanup_error
                .as_ref()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied);
            assert!(
                permission_denied,
                "restore failure was not a retained EPERM"
            );
            std::fs::write(&release_path, b"release").expect("authorize foreground release");
            let foreign_status = tokio::time::timeout(Duration::from_secs(2), foreign.wait())
                .await
                .expect("foreign foreground helper wait bound")
                .expect("wait registered foreign foreground helper");
            assert!(
                foreign_status.success(),
                "foreign helper failed: {foreign_status:?}"
            );
            wait_terminal_ownership_released().await;
            let cleanup_cleared = jackin_process_directory::foreground_cleanup_error().is_none();
            assert!(
                cleanup_cleared,
                "foreground cleanup error remained after release"
            );
            let after = wait_terminal_restored(&before).await;
            let attributes_restored = before.attributes == after.attributes;
            let signal_mask_restored = before.signal_mask == after.signal_mask;
            println!("RESTORE_FAILURE=true");
            println!(
                "FOREGROUND_CLEANUP_ERROR_RETAINED={}",
                cleanup_error.is_some()
            );
            println!("RESTORE_ERROR_PERMISSION_DENIED={permission_denied}");
            println!("OWNERSHIP_RETAINED_DURING_FAILURE={ownership_retained}");
            println!("FOREIGN_FOREGROUND_PGID={}", current.foreground_pgid);
            println!("FOREGROUND_CLEANUP_ERROR_CLEARED={cleanup_cleared}");
            println!(
                "EXTERNAL_SCOPE_RELEASED={}",
                !jackin_diagnostics::rich_terminal_owned()
            );
            println!("ATTRIBUTES_RESTORED={attributes_restored}");
            println!("SIGNAL_MASK_RESTORED={signal_mask_restored}");
            println!("RESTORE_RELEASED=true");
            assert_eq!(before, after);
        }
        _ => panic!("unknown interactive fixture mode: {mode}"),
    }
}
