//! Integration coverage for the `JACKIN_DEBUG` env-backed `--debug` flag.
//!
//! `unsafe_code = "forbid"` rules out in-process `std::env::set_var` for
//! testing env-driven clap parsing, so each test spawns the real binary
//! with an explicit env to control resolution. clap's value semantics
//! for `ArgAction::SetTrue` env vars (truthy strings → true, "0" / "false"
//! / "no" / "off" / empty → false, unset → default) are clap's contract
//! and not retested here; what we lock in is *our* binding — that
//! `JACKIN_DEBUG` is wired to `--debug` on every command that takes it.

use assert_cmd::Command;
use predicates::prelude::*;

/// `--help` output for any flag with `env = "X"` includes `[env: X=]`.
/// These tests prove our binding by checking that annotation appears
/// next to the `--debug` description on every entry point that has
/// `--debug`. A typo (e.g. `JACKING_DEBUG`) fails them.
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
        // ConsoleArgs is `#[command(flatten)]`'d into `Cli`, so `jackin
        // --help` shows the same `--debug` flag with the same env binding
        // as `jackin console --help`.
        Command::cargo_bin("jackin")
            .unwrap()
            .env_remove("JACKIN_DEBUG")
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("[env: JACKIN_DEBUG="));
    }
}

/// `jackin console` on a non-TTY exits with the
/// `CONSOLE_REQUIRES_TTY_ERROR` regardless of `--debug` state. The error
/// fires after CLI parsing, so a successful exit-1 with that exact
/// message proves clap parsed the args without rejecting the env var
/// (typos like `JACKIN_DEBUG=garbage` here would not actually be
/// rejected because `FalseyValueParser` accepts any string, but a wrong
/// *binding* — e.g. attaching `env = "JACKIN_DEBUG"` to a non-bool flag
/// — would fail parsing and produce a different error).
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
                "jackin' console requires an interactive terminal",
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
                "jackin' console requires an interactive terminal",
            ));
    }

    #[test]
    fn jackin_debug_empty_does_not_break_console_parse() {
        // Empty string is `FalseyValueParser`-falsy and a common way
        // env vars get passed through CI / shell pipelines. Lock in
        // that it parses cleanly rather than tripping a value-parser
        // error.
        Command::cargo_bin("jackin")
            .unwrap()
            .env("JACKIN_DEBUG", "")
            .arg("console")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "jackin' console requires an interactive terminal",
            ));
    }
}
