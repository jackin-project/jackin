// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn secret_references_reject_literals_and_interpolation() {
    for value in ["$TOKEN", "${TOKEN_2}", "op://Vault/Item/key"] {
        assert!(valid_secret_reference(value));
    }
    for value in [
        "secret",
        "$",
        "${}",
        "$1TOKEN",
        "$TOKEN/secret",
        "prefix${TOKEN}",
        "${TOKEN",
    ] {
        assert!(!valid_secret_reference(value));
    }
}

#[test]
fn listing_redacts_secret_and_endpoint() {
    let account = AccountConfig {
        enabled: true,
        name: "Work".into(),
        provider: AiProvider::OpenAi,
        credential: AccountCredential::ApiKey {
            value: EnvValue::Plain("SECRET".into()),
            base_url: Some("https://SECRET.example".into()),
            model: None,
        },
    };
    assert!(!account_row("work", &account).contains("SECRET"));
}

#[test]
fn account_commands_persist_and_revoke_workspace_access() {
    use crate::cli::{Cli, Command};
    use clap::Parser;
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let command = Cli::try_parse_from([
        "jackin",
        "account",
        "add",
        "work",
        "--name",
        "Work account",
        "--provider",
        "openai",
        "--api-key",
        "--secret-ref",
        "$WORK_KEY",
    ])
    .unwrap()
    .command
    .unwrap();
    let Command::Account(command) = command else {
        panic!("account command");
    };
    handle(command, &config, &paths).unwrap();
    config = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(config.accounts["work"].name, "Work account");
    let workspace = WorkspaceName::parse("app").unwrap();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .create_workspace(
            &workspace,
            jackin_config::WorkspaceConfig {
                version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
                workdir: "/workspace".into(),
                mounts: vec![jackin_config::MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace".into(),
                    readonly: false,
                    isolation: jackin_core::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    config = editor.save().unwrap();
    handle_workspace(
        WorkspaceAccountCommand::Assign {
            workspace: "app".into(),
            account: "work".into(),
        },
        &config,
        &paths,
    )
    .unwrap();
    config = AppConfig::load_or_init(&paths).unwrap();
    handle_workspace(
        WorkspaceAccountCommand::Select {
            workspace: "app".into(),
            account: Some("work".into()),
            agent: jackin_core::Agent::Codex,
            role: None,
            clear: false,
        },
        &config,
        &paths,
    )
    .unwrap();
    config = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(
        config.workspaces["app"].account_bindings[&jackin_core::Agent::Codex],
        "work"
    );
    handle_workspace(
        WorkspaceAccountCommand::Unassign {
            workspace: "app".into(),
            account: "work".into(),
        },
        &config,
        &paths,
    )
    .unwrap();
    config = AppConfig::load_or_init(&paths).unwrap();
    assert!(config.workspaces["app"].accounts.is_empty());
    assert!(config.workspaces["app"].account_bindings.is_empty());
    handle(
        AccountCommand::Remove { id: "work".into() },
        &config,
        &paths,
    )
    .unwrap();
    config = AppConfig::load_or_init(&paths).unwrap();
    assert!(!config.accounts.contains_key("work"));
}
