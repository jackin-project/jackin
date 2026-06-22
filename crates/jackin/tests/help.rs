//! Integration tests for `jackin help [COMMAND]...`.
//!
//! These tests spawn the real binary with a pipe (non-TTY), so `man`
//! and pagers output to stdout without blocking. The display chain
//! (man -> less/more -> raw stdout) always produces output — tests
//! check exit code and non-emptiness, not which fallback fired.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_with_no_args_exits_zero_and_produces_output() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("help")
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn help_config_exits_zero() {
    Command::cargo_bin("jackin")
        .unwrap()
        .args(["help", "config"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn help_config_auth_exits_zero_and_mentions_auth() {
    Command::cargo_bin("jackin")
        .unwrap()
        .args(["help", "config", "auth"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth").or(predicate::str::contains("Auth")));
}

#[test]
fn help_unknown_subcommand_exits_nonzero() {
    Command::cargo_bin("jackin")
        .unwrap()
        .args(["help", "doesnotexist"])
        .assert()
        .failure();
}

#[test]
fn help_appears_in_root_help_listing() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("\n  help "));
}

#[test]
fn root_help_footer_mentions_jackin_help() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("jackin help <command>"));
}
