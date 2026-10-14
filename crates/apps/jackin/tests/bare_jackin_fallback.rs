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
            "jackin❯ console requires an interactive terminal",
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
