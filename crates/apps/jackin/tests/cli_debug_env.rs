use assert_cmd::Command;
use predicates::prelude::*;

mod help_annotation {
    use super::*;

    #[test]
    fn load_help_advertises_jackin_debug() {
        Command::cargo_bin("jackin")
            .unwrap()
            .env_remove("JACKIN_DEBUG")
            .args(["load", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("[env: JACKIN_DEBUG="));
    }

    #[test]
    fn console_help_advertises_jackin_debug() {
        Command::cargo_bin("jackin")
            .unwrap()
            .env_remove("JACKIN_DEBUG")
            .args(["console", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("[env: JACKIN_DEBUG="));
    }

    #[test]
    fn top_level_help_advertises_jackin_debug() {
        Command::cargo_bin("jackin")
            .unwrap()
            .env_remove("JACKIN_DEBUG")
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("[env: JACKIN_DEBUG="));
    }
}

mod env_does_not_break_parsing {
    use super::*;

    #[test]
    fn jackin_debug_truthy_does_not_break_console_parse() {
        Command::cargo_bin("jackin")
            .unwrap()
            .env("JACKIN_DEBUG", "1")
            .arg("console")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "jackin❯ console requires an interactive terminal",
            ));
    }

    #[test]
    fn jackin_debug_falsy_does_not_break_console_parse() {
        Command::cargo_bin("jackin")
            .unwrap()
            .env("JACKIN_DEBUG", "0")
            .arg("console")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "jackin❯ console requires an interactive terminal",
            ));
    }

    #[test]
    fn jackin_debug_empty_does_not_break_console_parse() {
        Command::cargo_bin("jackin")
            .unwrap()
            .env("JACKIN_DEBUG", "")
            .arg("console")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "jackin❯ console requires an interactive terminal",
            ));
    }
}
