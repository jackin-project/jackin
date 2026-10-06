// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Installed executable boundaries: a missing service is an explicit failure.

use std::fs;
use std::path::Path;
use std::time::Duration;

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

#[test]
fn isolated_cli_missing_sibling_fails_with_installation_diagnostic() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path();
    initialize_empty_config(home)?;
    let bin = home.join("bin");
    fs::create_dir(&bin)?;
    let executable = bin.join("jackin");
    fs::copy(env!("CARGO_BIN_EXE_jackin"), &executable)?;
    assert!(!bin.join("jackin-usage-broker").exists());

    isolated_command(&executable, home)
        .args(["usage", "--format", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "cannot start usage broker executable",
        ))
        .stderr(predicate::str::contains("jackin-usage-broker"))
        .stderr(predicate::str::contains(
            "reinstall the complete jackin package",
        ));
    assert!(!home.join("state/data/usage-broker/run/leader.pid").exists());
    Ok(())
}

#[test]
fn missing_broker_override_preserves_failed_path() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path();
    initialize_empty_config(home)?;
    let missing = home.join("missing-broker");
    isolated_command(Path::new(env!("CARGO_BIN_EXE_jackin")), home)
        .env("JACKIN_USAGE_BROKER_BIN", &missing)
        .args(["usage", "--format", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(missing.to_string_lossy().as_ref()))
        .stderr(predicate::str::contains(
            "cannot start usage broker executable",
        ));
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
