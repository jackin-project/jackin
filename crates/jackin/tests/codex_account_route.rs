// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

//! A fake-only proof that the normal CLI routes an explicit Codex home through
//! discovery, account authorization, and role-scoped selection.

use std::{fs, path::Path, time::Duration};

use assert_cmd::Command;
use jackin_config::{AccountCredential, AiProvider, AppConfig, WorkspaceConfig, resolve_launch};
use jackin_core::{Agent, WorkspaceName};
use predicates::prelude::*;

fn command(home: &Path, codex_home: &Path) -> anyhow::Result<Command> {
    let mut command = Command::cargo_bin("jackin")?;
    command
        .env_clear()
        .env("HOME", home)
        .env("CODEX_HOME", codex_home)
        .env("JACKIN_HOME_DIR", home.join(".jackin"))
        .env("JACKIN_CONFIG_DIR", home.join(".config/jackin"))
        .env("PATH", "/usr/bin:/bin")
        .current_dir(home)
        .timeout(Duration::from_secs(20))
        .arg("--debug");
    Ok(command)
}

fn write_codex_auth(directory: &Path, marker: &str) -> anyhow::Result<()> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("auth.json"),
        format!(r#"{{"tokens":{{"access_token":"{marker}"}}}}"#),
    )?;
    Ok(())
}

fn registry(home: &Path) -> anyhow::Result<AppConfig> {
    Ok(toml::from_str(&fs::read_to_string(
        home.join(".config/jackin/config.toml"),
    )?)?)
}

fn workspace(home: &Path) -> anyhow::Result<WorkspaceConfig> {
    Ok(toml::from_str(&fs::read_to_string(
        home.join(".config/jackin/workspaces/route.toml"),
    )?)?)
}

fn selected_codex_account(
    config: &AppConfig,
    workspace: &WorkspaceConfig,
    workspace_name: &WorkspaceName,
    role: &str,
) -> anyhow::Result<(String, AiProvider, std::path::PathBuf)> {
    let mut config = config.clone();
    config
        .workspaces
        .insert(workspace_name.as_str().to_owned(), workspace.clone());
    let instances = resolve_launch(
        &config,
        Some(workspace_name),
        role,
        None,
        Some(Agent::Codex),
    )?;
    anyhow::ensure!(instances.len() == 1, "expected one selected Codex instance");
    let instance = &instances[0];
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("resolved account is missing"))?;
    let AccountCredential::Profile {
        agent: Agent::Codex,
        directory,
        ..
    } = &account.credential
    else {
        anyhow::bail!("selected Codex account did not resolve to a profile");
    };
    Ok((
        instance.account_id.clone(),
        account.provider,
        directory.clone(),
    ))
}

#[test]
fn explicit_codex_home_keeps_discovery_and_role_selection_on_one_source() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let home = temporary.path().join("operator-home");
    fs::create_dir_all(&home)?;
    let default_home = home.join(".codex");
    let explicit_home = home.join("codex-explicit");
    let second_profile = home.join("codex-second-profile");
    write_codex_auth(&default_home, "synthetic-default-token")?;
    write_codex_auth(&explicit_home, "synthetic-explicit-token")?;
    write_codex_auth(&second_profile, "synthetic-second-token")?;
    fs::create_dir_all(home.join(".claude"))?;
    fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"synthetic-claude-token"}}"#,
    )?;
    // The shell declaration and the process override name the same source.
    // The scan should keep the default discovery ID and deduplicate the
    // duplicate shell candidate by its canonical source fingerprint.
    fs::write(
        home.join(".zshrc"),
        format!("CODEX_HOME={}\n", explicit_home.display()),
    )?;

    command(&home, &explicit_home)?
        .args(["account", "scan"])
        .assert()
        .success()
        .stdout(predicate::str::contains("synthetic-").not())
        .stderr(predicate::str::contains("synthetic-").not());

    let imported = registry(&home)?;
    let default = imported
        .accounts
        .get("default-codex")
        .expect("explicit Codex home should be discovered as the default account");
    assert!(matches!(
        &default.credential,
        AccountCredential::Profile {
            agent: Agent::Codex,
            directory,
            ..
        } if directory == &explicit_home
    ));
    assert!(!imported.accounts.contains_key("custom-codex"));
    let rendered = format!("{imported:?}");
    assert!(!rendered.contains("synthetic-"));
    assert!(!rendered.contains(&default_home.display().to_string()));

    // An operator-registered second home remains distinct from the
    // environment-selected account.
    command(&home, &explicit_home)?
        .args([
            "account",
            "add",
            "codex-second",
            "--agent",
            "codex",
            "--directory",
        ])
        .arg(&second_profile)
        .assert()
        .success();
    let project = home.join("project");
    fs::create_dir_all(&project)?;
    command(&home, &explicit_home)?
        .args([
            "workspace",
            "create",
            "route",
            "--workdir",
            "/workspace",
            "--mount",
        ])
        .arg(format!("{}:/workspace", project.display()))
        .args(["--default-agent", "codex"])
        .assert()
        .success();
    // The fake-only route test needs two registered roles so its account
    // bindings remain independent. No repository access is required.
    let config_path = home.join(".config/jackin/config.toml");
    let mut config_text = fs::read_to_string(&config_path)?;
    config_text.push_str(
        "\n[roles.alternate-role]\ngit = \"https://roles.invalid/jackin-alternate-role.git\"\n",
    );
    fs::write(&config_path, config_text)?;
    for account in ["default-codex", "codex-second"] {
        command(&home, &explicit_home)?
            .args(["workspace", "account", "assign", "route", account])
            .assert()
            .success();
    }
    command(&home, &explicit_home)?
        .args([
            "workspace",
            "account",
            "select",
            "route",
            "default-codex",
            "--agent",
            "codex",
            "--role",
            "the-architect",
        ])
        .assert()
        .success();
    command(&home, &explicit_home)?
        .args([
            "workspace",
            "account",
            "select",
            "route",
            "codex-second",
            "--agent",
            "codex",
            "--role",
            "alternate-role",
        ])
        .assert()
        .success();

    let selected = registry(&home)?;
    let selected_workspace = workspace(&home)?;
    let workspace_name = WorkspaceName::parse("route")?;
    let (default_id, default_provider, default_source) = selected_codex_account(
        &selected,
        &selected_workspace,
        &workspace_name,
        "the-architect",
    )?;
    assert_eq!(default_id, "default-codex");
    assert_eq!(default_provider, AiProvider::OpenAi);
    assert_eq!(default_source, explicit_home);

    let (alternate_id, alternate_provider, alternate_source) = selected_codex_account(
        &selected,
        &selected_workspace,
        &workspace_name,
        "alternate-role",
    )?;
    assert_eq!(alternate_id, "codex-second");
    assert_eq!(alternate_provider, AiProvider::OpenAi);
    assert_eq!(alternate_source, second_profile.canonicalize()?);
    Ok(())
}
