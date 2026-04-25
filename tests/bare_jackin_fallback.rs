//! Integration coverage for the bare-`jackin` / `jackin console` /
//! `jackin launch` dispatch.
//!
//! Each test spawns the real binary with a pipe (guaranteeing a
//! non-TTY stdout) and checks that the fallback behaviour matches the
//! contract encoded in [`jackin::cli::dispatch`]:
//!
//! - bare `jackin` on a non-TTY prints help and exits 0
//! - `jackin console` on a non-TTY errors and exits 1
//! - `jackin launch` on a non-TTY emits the deprecation warning first
//!   and then the same TTY-required error

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn bare_jackin_without_tty_prints_help_and_exits_zero() {
    Command::cargo_bin("jackin")
        .unwrap()
        .assert()
        .success()
        .stdout(predicate::str::contains("Operator's CLI for orchestrating"));
}

#[test]
fn bare_jackin_without_tty_does_not_emit_warning() {
    // Silent fallback: operator did not ask for the console by name.
    Command::cargo_bin("jackin")
        .unwrap()
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[test]
fn console_subcommand_without_tty_errors() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("console")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "jackin' console requires an interactive terminal",
        ));
}

#[test]
fn launch_subcommand_without_tty_emits_deprecation_then_errors() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("launch")
        .assert()
        .failure()
        .stderr(predicate::str::contains("`jackin launch` is deprecated"))
        .stderr(predicate::str::contains(
            "jackin' console requires an interactive terminal",
        ));
}

#[test]
fn help_flag_exits_normally() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Operator's CLI"));
}

#[test]
fn version_flag_exits_normally() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("jackin"));
}
