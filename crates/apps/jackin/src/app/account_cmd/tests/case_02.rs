// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        &startup_bootstrap(),
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
