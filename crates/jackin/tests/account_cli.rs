// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Real executable coverage for account onboarding and workspace authorization.

use std::{fs, path::Path, time::Duration};

use assert_cmd::Command;
use jackin_config::{AccountCredential, AppConfig, WorkspaceConfig};
use jackin_core::{Agent, EnvValue};
use predicates::prelude::*;

fn command(home: &Path) -> anyhow::Result<Command> {
    let mut command = Command::cargo_bin("jackin")?;
    command
        .env_clear()
        .env("HOME", home)
        .env("JACKIN_HOME_DIR", home.join(".jackin"))
        .env("JACKIN_CONFIG_DIR", home.join(".config/jackin"))
        .env("PATH", "/usr/bin:/bin")
        .current_dir(home)
        .timeout(Duration::from_secs(20))
        .arg("--debug");
    Ok(command)
}

fn registry(home: &Path) -> anyhow::Result<AppConfig> {
    Ok(toml::from_str(&fs::read_to_string(
        home.join(".config/jackin/config.toml"),
    )?)?)
}

fn workspace(home: &Path) -> anyhow::Result<WorkspaceConfig> {
    Ok(toml::from_str(&fs::read_to_string(
        home.join(".config/jackin/workspaces/project.toml"),
    )?)?)
}

#[test]
fn account_cli_onboards_and_enforces_workspace_assignments() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let home = temporary.path();
    fs::create_dir(home.join(".codex"))?;
    fs::write(
        home.join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"synthetic-default-token"}}"#,
    )?;
    // A synthetic Claude file prevents querying the real user's macOS Keychain.
    fs::create_dir(home.join(".claude"))?;
    fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"synthetic-claude-token"}}"#,
    )?;
    command(home)?
        .args(["account", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default-codex"))
        .stdout(predicate::str::contains("synthetic-default-token").not())
        .stderr(predicate::str::contains("synthetic-default-token").not());
    assert!(matches!(
        &registry(home)?.accounts["default-codex"].credential,
        AccountCredential::Profile { agent: Agent::Codex, directory }
            if directory == &home.join(".codex")
    ));

    let profile = home.join("codex-work");
    fs::create_dir(&profile)?;
    fs::write(
        profile.join("auth.json"),
        r#"{"OPENAI_API_KEY":"synthetic-profile-key"}"#,
    )?;
    command(home)?
        .args(["account", "add", "work", "--agent", "codex", "--directory"])
        .arg(&profile)
        .assert()
        .success();
    assert!(matches!(
        &registry(home)?.accounts["work"].credential,
        AccountCredential::Profile { agent: Agent::Codex, directory }
            if directory == &profile.canonicalize()?
    ));
    command(home)?
        .args([
            "account",
            "add",
            "api",
            "--provider",
            "openai",
            "--api-key",
            "--stdin",
        ])
        .write_stdin("synthetic-stdin-key\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("synthetic-stdin-key").not())
        .stderr(predicate::str::contains("synthetic-stdin-key").not());
    assert!(matches!(
        &registry(home)?.accounts["api"].credential,
        AccountCredential::ApiKey { value: EnvValue::Plain(value), .. }
            if value == "synthetic-stdin-key"
    ));

    command(home)?
        .args(["account", "default", "api", "--agent", "codex"])
        .assert()
        .success();
    assert_eq!(registry(home)?.account_bindings[&Agent::Codex], "api");
    command(home)?
        .args(["account", "disable", "api"])
        .assert()
        .success();
    assert!(!registry(home)?.accounts["api"].enabled);
    assert!(!registry(home)?.account_bindings.contains_key(&Agent::Codex));
    command(home)?
        .args(["account", "default", "api", "--agent", "codex"])
        .assert()
        .failure();
    command(home)?
        .args(["account", "enable", "api"])
        .assert()
        .success();
    assert!(registry(home)?.accounts["api"].enabled);
    command(home)?
        .args([
            "account",
            "add",
            "duplicate",
            "--provider",
            "openai",
            "--api-key",
            "--stdin",
        ])
        .write_stdin("synthetic-stdin-key\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("synthetic-stdin-key").not());
    assert!(!registry(home)?.accounts.contains_key("duplicate"));

    assert_workspace_assignment_lifecycle(home)?;

    command(home)?
        .args(["account", "remove", "default-codex"])
        .assert()
        .success();
    command(home)?
        .args(["account", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default-codex").not());
    command(home)?.args(["account", "scan"]).assert().success();
    assert!(registry(home)?.accounts.contains_key("default-codex"));
    Ok(())
}

fn assert_workspace_assignment_lifecycle(home: &Path) -> anyhow::Result<()> {
    let mount = format!("{}:/workspace", home.display());
    command(home)?
        .args([
            "workspace",
            "create",
            "project",
            "--workdir",
            "/workspace",
            "--mount",
            &mount,
        ])
        .assert()
        .success();
    assert!(workspace(home)?.accounts.is_empty());
    command(home)?
        .args([
            "workspace",
            "account",
            "select",
            "project",
            "api",
            "--agent",
            "codex",
        ])
        .assert()
        .failure();
    assert!(workspace(home)?.account_bindings.is_empty());
    command(home)?
        .args(["workspace", "account", "assign", "project", "api"])
        .assert()
        .success();
    command(home)?
        .args([
            "workspace",
            "account",
            "select",
            "project",
            "api",
            "--agent",
            "codex",
        ])
        .assert()
        .success();
    command(home)?
        .args(["workspace", "account", "list", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("codex -> api"))
        .stdout(predicate::str::contains("synthetic-stdin-key").not());
    assert_eq!(workspace(home)?.account_bindings[&Agent::Codex], "api");
    command(home)?
        .args(["account", "remove", "api"])
        .assert()
        .success();
    assert!(!registry(home)?.accounts.contains_key("api"));
    assert!(workspace(home)?.accounts.is_empty());
    assert!(workspace(home)?.account_bindings.is_empty());

    Ok(())
}
