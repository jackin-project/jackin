// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Installed executable boundaries: passive reads stay passive, while explicit
//! service startup reports a missing broker executable.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::Context as _;
use assert_cmd::Command;
use predicates::prelude::*;

fn isolated_command(executable: &Path, home: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .env_clear()
        .env("HOME", home)
        .env("JACKIN_HOME_DIR", home.join("state"))
        .env("JACKIN_CONFIG_DIR", home.join("config"))
        .env("PATH", home.join("bin"))
        .current_dir(home)
        .timeout(Duration::from_secs(20));
    command
}

fn initialize_empty_config(home: &Path) -> anyhow::Result<()> {
    let config = jackin_config::AppConfig {
        bootstrap: Some(jackin_config::BootstrapState::initialized()),
        ..Default::default()
    };
    fs::create_dir_all(home.join("config"))?;
    fs::write(home.join("config/config.toml"), toml::to_string(&config)?)?;
    Ok(())
}

fn assert_broker_unavailable(
    output: &std::process::Output,
    message_fragment: &str,
) -> anyhow::Result<serde_json::Value> {
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("expected structured usage error JSON")?;
    assert_eq!(value["version"], 1);
    assert_eq!(value["error"]["code"], "broker_unavailable");
    let message = value["error"]["message"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("usage error message must be a string"))?;
    assert!(message.contains(message_fragment), "{message}");
    Ok(value)
}

fn assert_interaction_required(output: &std::process::Output) -> anyhow::Result<()> {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("expected structured usage error JSON")?;
    assert_eq!(value["version"], 1);
    assert_eq!(value["error"]["code"], "interaction_required");
    Ok(())
}

#[test]
fn service_start_missing_sibling_reports_install_hint() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path();
    initialize_empty_config(home)?;
    let bin = home.join("bin");
    fs::create_dir(&bin)?;
    let executable = bin.join("jackin");
    fs::copy(env!("CARGO_BIN_EXE_jackin"), &executable)?;
    assert!(!bin.join("jackin-usage-broker").exists());
    let data_dir = home.join("isolated-usage-state");

    let output = isolated_command(&executable, home)
        .args([
            "usage",
            "service",
            "start",
            "--format",
            "json",
            "--data-dir",
        ])
        .arg(&data_dir)
        .output()?;
    let sibling = bin.join("jackin-usage-broker");
    let value =
        assert_broker_unavailable(&output, "cannot start local-only usage broker executable")?;
    let message = value["error"]["message"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("usage error message must be a string"))?;
    assert!(
        message.contains(sibling.to_string_lossy().as_ref()),
        "{message}"
    );
    assert!(message.contains("reinstall the complete jackin package"));
    assert!(!data_dir.join("usage-broker/run").exists());
    Ok(())
}

#[test]
fn passive_usage_reads_do_not_activate_missing_broker_override() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path();
    initialize_empty_config(home)?;
    let missing = home.join("missing-broker");
    let data_dir = home.join("isolated-usage-state");
    let executable = Path::new(env!("CARGO_BIN_EXE_jackin"));

    for (args, expected_message) in [
        (vec!["usage"], "cached usage projection is unavailable"),
        (
            vec!["usage", "doctor", "--provider", "claude", "--unattended"],
            "host usage broker is unavailable or returned an incompatible response",
        ),
    ] {
        let output = isolated_command(executable, home)
            .env("JACKIN_USAGE_BROKER_BIN", &missing)
            .args(args)
            .args(["--format", "json", "--data-dir"])
            .arg(&data_dir)
            .output()?;
        let value = assert_broker_unavailable(&output, expected_message)?;
        let message = value["error"]["message"].as_str().unwrap_or_default();
        assert!(!message.contains(missing.to_string_lossy().as_ref()));
        assert!(!message.contains("cannot start local-only usage broker executable"));
        assert!(!data_dir.join("usage-broker/run").exists());
    }
    Ok(())
}

#[test]
fn explicit_service_start_preserves_missing_broker_override_path() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path();
    initialize_empty_config(home)?;
    let missing = home.join("missing-broker");
    let data_dir = home.join("isolated-usage-state");
    let output = isolated_command(Path::new(env!("CARGO_BIN_EXE_jackin")), home)
        .env("JACKIN_USAGE_BROKER_BIN", &missing)
        .args([
            "usage",
            "service",
            "start",
            "--format",
            "json",
            "--data-dir",
        ])
        .arg(&data_dir)
        .output()?;
    let value =
        assert_broker_unavailable(&output, "cannot start local-only usage broker executable")?;
    let message = value["error"]["message"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("usage error message must be a string"))?;
    assert!(
        message.contains(missing.to_string_lossy().as_ref()),
        "{message}"
    );
    assert!(message.contains("reinstall the complete jackin package"));
    assert!(!data_dir.join("usage-broker/run").exists());
    Ok(())
}

#[test]
fn headless_binding_and_policy_approval_reject_before_broker_activation() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path();
    initialize_empty_config(home)?;
    let missing = home.join("missing-broker");
    let data_dir = home.join("isolated-usage-state");
    let executable = Path::new(env!("CARGO_BIN_EXE_jackin"));

    for args in [
        vec![
            "binding",
            "confirm",
            "--provider",
            "claude",
            "--account",
            "fixture-account",
            "--operator-label",
            "offline fixture",
            "--confirm",
        ],
        vec![
            "policy",
            "approve",
            "--binding",
            "fixture-binding",
            "--binding-revision",
            "1",
            "--goal",
            "fixture-goal",
            "--policy",
            "strict-sgd",
            "--budget-sgd",
            "50",
            "--operator-label",
            "offline fixture",
            "--confirm",
        ],
    ] {
        let output = isolated_command(executable, home)
            .env("JACKIN_USAGE_BROKER_BIN", &missing)
            .args(["usage", "--format", "json", "--data-dir"])
            .arg(&data_dir)
            .args(args)
            .output()?;
        assert_interaction_required(&output)?;
        assert!(
            !data_dir.join("usage-broker/run").exists(),
            "headless operator action activated a broker"
        );
        let message = String::from_utf8_lossy(&output.stdout);
        assert!(!message.contains(missing.to_string_lossy().as_ref()));
    }
    Ok(())
}

#[test]
fn packaged_broker_version_needs_no_service_configuration() {
    Command::new(env!("CARGO_BIN_EXE_jackin-usage-broker"))
        .env_clear()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("jackin-usage-broker {}\n", env!("JACKIN_VERSION")))
        .stderr(predicate::str::is_empty());
}
