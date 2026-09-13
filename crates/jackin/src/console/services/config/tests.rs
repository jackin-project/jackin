// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{WorkspaceSaveInput, WorkspaceSaveMode, save_workspace};
use jackin_config::{
    AccountConfig, AccountCredential, AiProvider, AppConfig, CURRENT_WORKSPACE_VERSION, EnvValue,
    MountConfig, MountIsolation, WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_core::{Agent, JackinPaths};

fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
    std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
}

#[test]
fn save_workspace_persists_and_clears_account_assignments_and_bindings() {
    let tmp = tempfile::tempdir().unwrap();
    let mount_src = tmp.path().join("repo");
    std::fs::create_dir(&mount_src).unwrap();
    let original = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/proj".to_owned(),
        mounts: vec![MountConfig {
            src: mount_src.display().to_string(),
            dst: "/workspace/proj".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    };
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config
        .workspaces
        .insert("proj".to_owned(), original.clone());
    config.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: EnvValue::Plain("test-key".into()),
                base_url: None,
                model: None,
            },
        },
    );
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut pending = original.clone();
    pending.accounts.push("work".into());
    pending
        .account_bindings
        .insert(Agent::Claude, "work".into());
    pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: [(Agent::Claude, "work".into())].into(),
            ..Default::default()
        },
    );

    let saved = save_workspace(
        &paths,
        WorkspaceSaveInput {
            mode: WorkspaceSaveMode::Edit {
                original_name: "proj".to_owned(),
                pending_name: None,
                effective_removals: Vec::new(),
            },
            original: &original,
            pending: &pending,
        },
    )
    .unwrap();

    let reloaded = saved.config.workspaces.get("proj").unwrap();
    assert_eq!(reloaded.accounts, ["work"]);
    assert_eq!(
        reloaded
            .account_bindings
            .get(&Agent::Claude)
            .map(String::as_str),
        Some("work")
    );
    assert_eq!(
        reloaded.roles["smith"]
            .account_bindings
            .get(&Agent::Claude)
            .map(String::as_str),
        Some("work")
    );
    let mut cleared = reloaded.clone();
    cleared.accounts.clear();
    cleared.account_bindings.clear();
    cleared.roles.clear();
    save_workspace(
        &paths,
        WorkspaceSaveInput {
            mode: WorkspaceSaveMode::Edit {
                original_name: "proj".to_owned(),
                pending_name: None,
                effective_removals: Vec::new(),
            },
            original: reloaded,
            pending: &cleared,
        },
    )
    .unwrap();

    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let workspace = reloaded.workspaces.get("proj").unwrap();
    assert!(workspace.accounts.is_empty());
    assert!(workspace.account_bindings.is_empty());
    assert!(workspace.roles.is_empty());

    let out = workspace_file_contents(&paths, "proj");
    assert!(!out.contains("work\""), "{out}");
}
