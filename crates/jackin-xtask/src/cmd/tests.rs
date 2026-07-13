use std::ffi::OsStr;
use std::process::Command;

use super::{display_command, output, output_string, run, shell_quote};

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
fn run_nonzero_status_errors() {
    let mut cmd = Command::new("sh");
    cmd.args(["-lc", "exit 2"]);
    let err = run(&mut cmd).expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("failed"), "msg={msg}");
}
