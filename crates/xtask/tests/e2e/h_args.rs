// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::{Command, Output};

use super::support::capture;

fn xtask(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"));
    command
        .arg("e2e")
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent-home");
    capture(&mut command)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_e2e_command_parses_and_the_stub_is_gone() {
    let help = capture(Command::new(env!("CARGO_BIN_EXE_jackin-xtask")).arg("--help"));
    let text = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(
        text.contains("e2e") && text.contains("Run the sentinel journey"),
        "{text}"
    );
    assert!(!text.contains("not built yet"));

    for args in [
        vec!["--gateway", "g", "--role", "../r", "--jackin", "/bin/echo"],
        vec![
            "--gateway",
            "g",
            "--role",
            "../r",
            "--engine-port",
            "2376",
            "--engine-certs",
            "d",
            "--jackin",
            "/bin/echo",
        ],
        vec!["--gateway", "g", "--sweep", "--jackin", "/bin/echo"],
    ] {
        let output = xtask(&args);
        let message = stderr(&output);
        assert!(
            message.starts_with("xtask:"),
            "{args:?} parses and reaches the runner: {message}"
        );
        assert_eq!(output.status.code(), Some(2), "{args:?}: {message}");
    }

    for (args, needle) in [
        (vec!["--role", "r"], "--gateway"),
        (vec!["--gateway", "g"], "--role"),
        (
            vec!["--gateway", "g", "--role", "r", "--engine-port", "2376"],
            "--engine-certs",
        ),
        (
            vec!["--gateway", "g", "--role", "r", "--engine-certs", "d"],
            "--engine-port",
        ),
        (
            vec![
                "--gateway",
                "g",
                "--role",
                "r",
                "--engine-port",
                "0",
                "--engine-certs",
                "d",
            ],
            "invalid value",
        ),
        (vec!["--gateway", "g", "--role", "r", "--bogus"], "--bogus"),
        (
            vec!["--gateway", "g", "--role", "r", "--only", "J11"],
            "--only",
        ),
        (vec![], "--gateway"),
    ] {
        let output = xtask(&args);
        let message = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {message}");
        assert!(
            message.starts_with("error:") && message.contains(needle),
            "{args:?}: {message}"
        );
    }
}
