// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::{Command, Output};

use jackin_xtask::{ProductionError, e2e_command};
use serde_json::json;

use super::scenario::Scenario;
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

fn prepared(scenario: &Scenario) -> jackin_xtask::Boundaries {
    scenario.state().tag.clone_from(&scenario.tag);
    scenario.boundaries().0
}

#[tokio::test]
async fn the_command_glue_maps_every_outcome_to_its_exit_code() {
    let pass = Scenario::new().await;
    let boundaries = prepared(&pass);
    assert_eq!(e2e_command(&pass.args, async { Ok(boundaries) }).await, 0);

    let failing = Scenario::new().await;
    failing.patch("J10", None, "/result", json!("FAIL"));
    failing.patch("J10", None, "/reason", json!("broken"));
    failing.patch("J10", None, "/checks/0/result", json!("FAIL"));
    let boundaries = prepared(&failing);
    assert_eq!(
        e2e_command(&failing.args, async { Ok(boundaries) }).await,
        1
    );

    let skipping = Scenario::new().await;
    skipping.patch("J08", None, "/checks/0/name", json!("environment"));
    skipping.patch("J08", None, "/checks/0/result", json!("SKIP"));
    skipping.patch("J08", None, "/result", json!("SKIP"));
    skipping.patch("J08", None, "/reason", json!("skipped"));
    skipping.patch("J08", None, "/facts/start_record/state", json!("absent"));
    let boundaries = prepared(&skipping);
    assert_eq!(
        e2e_command(&skipping.args, async { Ok(boundaries) }).await,
        3
    );

    let interrupted = Scenario::new().await;
    interrupted.knobs().cancel_at = Some("ps -eo".to_owned());
    let boundaries = prepared(&interrupted);
    assert_eq!(
        e2e_command(&interrupted.args, async { Ok(boundaries) }).await,
        130
    );

    let refused = Scenario::new().await;
    let code = e2e_command(&refused.args, async {
        Err(ProductionError::Setup("no gateway".to_owned()))
    })
    .await;
    assert_eq!(code, 2);
}
