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
fn scan_reference_variable_prints_only_validated_references() {
    let plain = |value: &str| AccountConfig {
        enabled: true,
        name: "probe".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::ApiKey {
            value: EnvValue::Plain(value.into()),
            base_url: None,
            model: None,
        },
    };
    assert_eq!(
        scan_reference_variable(&plain("$ANTHROPIC_API_KEY")),
        Some("ANTHROPIC_API_KEY")
    );
    assert_eq!(
        scan_reference_variable(&plain("${ANTHROPIC_API_KEY}")),
        Some("ANTHROPIC_API_KEY")
    );
    for shape in [
        "secret",
        "$",
        "${}",
        "$1TOKEN",
        "${TOKEN",
        "$TOKEN/secret",
        "prefix${TOKEN}",
        "op://vault/item/field",
        "${TOKEN}}",
    ] {
        assert_eq!(scan_reference_variable(&plain(shape)), None, "{shape}");
    }
    let profile = AccountConfig {
        enabled: true,
        name: "probe".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::Profile {
            agent: jackin_core::Agent::Claude,
            directory: "/tmp/probe".into(),
            xdg_roots: None,
            source_selector: None,
        },
    };
    assert_eq!(scan_reference_variable(&profile), None);
    let op_ref = AccountConfig {
        enabled: true,
        name: "probe".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::ApiKey {
            value: EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item-id/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
            base_url: None,
            model: None,
        },
    };
    // 1Password item IDs never reach operator output.
    assert_eq!(scan_reference_variable(&op_ref), None);
}

#[test]
fn scan_imports_discovered_profiles_once_with_bootstrap_naming() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    drop(AppConfig::load_or_init(&paths).unwrap());
    let claude_dir = paths.home_dir.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    handle(AccountCommand::Scan, &config, &paths).unwrap();
    let config = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(config.accounts["default-claude"].name, "Claude default");

    // Second scan dedupes: no suffixed clones.
    handle(AccountCommand::Scan, &config, &paths).unwrap();
    let config = AppConfig::load_or_init(&paths).unwrap();
    assert!(
        !config
            .accounts
            .keys()
            .any(|id| id.starts_with("default-claude-")),
        "{:?}",
        config.accounts.keys().collect::<Vec<_>>()
    );
}

#[test]
fn scan_seeds_zshrc_overrides_alongside_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    drop(AppConfig::load_or_init(&paths).unwrap());
    let override_dir = temp.path().join("codex-override");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join("auth.json"),
        r#"{"OPENAI_API_KEY":"fixture"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(&paths.home_dir).unwrap();
    std::fs::write(
        paths.home_dir.join(".zshrc"),
        format!(
            "CODEX_HOME={}\nSOME_API_KEY=$(some-helper)\n",
            override_dir.display()
        ),
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    handle(AccountCommand::Scan, &config, &paths).unwrap();
    let config = AppConfig::load_or_init(&paths).unwrap();
    let seeded = &config.accounts["custom-codex"];
    assert_eq!(seeded.name, "Codex custom");
    assert_eq!(seeded.source_directory(), Some(override_dir.as_path()));
}

#[test]
fn scan_persists_existing_account_model_and_endpoint_updates() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    drop(AppConfig::load_or_init(&paths).unwrap());
    let account = AccountConfig {
        enabled: true,
        name: "OpenAI".into(),
        provider: AiProvider::OpenAi,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("$OPENAI_API_KEY"),
            base_url: None,
            model: None,
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("openai-api-key", &account).unwrap();
    editor.save().unwrap();
    std::fs::write(
        paths.home_dir.join(".zshrc"),
        "OPENAI_MODEL=gpt-5\nOPENAI_BASE_URL=https://proxy.example/v1\n",
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    handle(AccountCommand::Scan, &config, &paths).unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    let AccountCredential::ApiKey {
        model, base_url, ..
    } = &config.accounts["openai-api-key"].credential
    else {
        panic!("expected API-key account");
    };
    assert_eq!(model.as_deref(), Some("gpt-5"));
    assert_eq!(base_url.as_deref(), Some("https://proxy.example/v1"));
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

#[test]
fn disabling_account_prunes_bindings_at_all_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    drop(AppConfig::load_or_init(&paths).unwrap());

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account(
            "work-1",
            &AccountConfig {
                enabled: true,
                name: "Work 1".into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::Profile {
                    agent: jackin_core::Agent::Claude,
                    directory: temp.path().join("profiles/work-1"),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        )
        .unwrap();
    editor
        .upsert_account(
            "work-2",
            &AccountConfig {
                enabled: true,
                name: "Work 2".into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::Profile {
                    agent: jackin_core::Agent::Claude,
                    directory: temp.path().join("profiles/work-2"),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        )
        .unwrap();
    let workspace = WorkspaceName::parse("project").unwrap();
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
                accounts: vec!["work-1".into(), "work-2".into()],
                ..Default::default()
            },
        )
        .unwrap();
    editor
        .set_account_binding(None, None, jackin_core::Agent::Claude, Some("work-1"))
        .unwrap();
    editor
        .set_account_binding(
            Some(&workspace),
            None,
            jackin_core::Agent::Claude,
            Some("work-1"),
        )
        .unwrap();
    editor
        .set_account_binding(
            Some(&workspace),
            Some("smith"),
            jackin_core::Agent::Claude,
            Some("work-1"),
        )
        .unwrap();
    let mut config = editor.save().unwrap();

    assert_eq!(
        config.account_bindings[&jackin_core::Agent::Claude],
        "work-1"
    );
    assert_eq!(
        config.workspaces["project"].account_bindings[&jackin_core::Agent::Claude],
        "work-1"
    );
    assert_eq!(
        config.workspaces["project"].roles["smith"].account_bindings[&jackin_core::Agent::Claude],
        "work-1"
    );

    handle(
        AccountCommand::Disable {
            id: "work-1".into(),
        },
        &config,
        &paths,
    )
    .unwrap();

    config = AppConfig::load_or_init(&paths).unwrap();
    assert!(!config.accounts["work-1"].enabled);
    assert!(config.account_bindings.is_empty());
    assert!(config.workspaces["project"].account_bindings.is_empty());
    assert!(
        config.workspaces["project"].roles["smith"]
            .account_bindings
            .is_empty()
    );

    let resolved = jackin_config::resolve_account(
        &config,
        jackin_core::Agent::Claude,
        Some(&workspace),
        "smith",
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolved.name, "Work 2");
}
