use std::ffi::OsStr;
use std::process::Command;
use std::time::{Duration, Instant};

use super::{
    display_command, output, output_string, output_timeout, output_timeout_bounded, run,
    run_streaming_timeout, shell_quote,
};

#[test]
fn shell_quote_leaves_plain_paths_bare() {
    assert_eq!(shell_quote(OsStr::new("cargo")), "cargo");
    assert_eq!(shell_quote(OsStr::new("/tmp/a_b-c:d+e")), "/tmp/a_b-c:d+e");
}

#[test]
fn shell_quote_wraps_spaces_and_quotes() {
    assert_eq!(shell_quote(OsStr::new("a b")), "'a b'");
    assert_eq!(shell_quote(OsStr::new("a'b")), "'a'\"'\"'b'");
}

#[test]
fn display_command_joins_program_and_args() {
    let mut cmd = Command::new("echo");
    cmd.arg("hi");
    assert_eq!(display_command(&cmd), "echo hi");
}

#[test]
fn output_success_captures_stdout() {
    let mut cmd = Command::new("echo");
    cmd.arg("hello-cmd");
    let out = output_string(&mut cmd).expect("echo should succeed");
    assert_eq!(out.trim(), "hello-cmd");
}

#[test]
fn output_failure_includes_program_and_stderr() {
    let mut cmd = Command::new("sh");
    cmd.args(["-lc", "echo err >&2; exit 3"]);
    let err = output(&mut cmd).expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("sh"), "msg={msg}");
}

#[test]
fn output_timeout_bounds_stalled_commands() {
    let mut cmd = Command::new("sleep");
    cmd.arg("2");
    let started = Instant::now();
    let err = output_timeout(&mut cmd, Duration::from_millis(20)).expect_err("should time out");
    assert!(format!("{err:#}").contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn run_nonzero_status_errors() {
    let mut cmd = Command::new("sh");
    cmd.args(["-lc", "exit 2"]);
    let err = run(&mut cmd).expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("failed"), "msg={msg}");
}

#[test]
fn bounded_output_accepts_exact_limit_and_rejects_stdout_or_stderr_overflow() {
    let mut exact = Command::new("sh");
    exact.args(["-c", "printf 1234"]);
    assert_eq!(
        output_timeout_bounded(&mut exact, Duration::from_secs(1), 4, 8).unwrap(),
        b"1234"
    );

    let mut stdout = Command::new("sh");
    stdout.args(["-c", "printf 12345"]);
    assert!(
        output_timeout_bounded(&mut stdout, Duration::from_secs(1), 4, 8)
            .unwrap_err()
            .to_string()
            .contains("bounded output capture")
    );

    let mut stderr = Command::new("sh");
    stderr.args(["-c", "printf 12345 >&2"]);
    assert!(
        output_timeout_bounded(&mut stderr, Duration::from_secs(1), 8, 4)
            .unwrap_err()
            .to_string()
            .contains("bounded output capture")
    );
}

#[test]
fn streaming_commands_have_a_hard_timeout() {
    let mut command = Command::new("sleep");
    command.arg("2");
    let started = Instant::now();
    let error = run_streaming_timeout(&mut command, Duration::from_millis(20)).unwrap_err();
    assert!(format!("{error:#}").contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(1));
}
