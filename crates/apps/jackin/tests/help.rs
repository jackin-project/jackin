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
fn help_workspace_account_exits_zero_and_produces_output() {
    Command::cargo_bin("jackin")
        .unwrap()
        .args(["help", "workspace", "account"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
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

#[test]
fn root_help_leads_with_brand_mark() {
    Command::cargo_bin("jackin")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("jackin❯"));
}
