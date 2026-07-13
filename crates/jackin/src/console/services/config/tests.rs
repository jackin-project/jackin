// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use super::{WorkspaceSaveInput, WorkspaceSaveMode, save_workspace};
use jackin_config::{
    AgentAuthConfig, AppConfig, CURRENT_WORKSPACE_VERSION, MountConfig, MountIsolation,
    WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_core::JackinPaths;

fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
    std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
}

#[test]
fn save_workspace_persists_and_clears_workspace_and_role_sync_source_dirs() {
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
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let workspace_source = PathBuf::from("/host/claude");
    let role_source = PathBuf::from("/host/codex");
    let mut pending = original.clone();
    pending.claude = Some(AgentAuthConfig {
        sync_source_dir: Some(workspace_source.clone()),
        ..Default::default()
    });
    pending.roles.insert(
        "smith".to_owned(),
        WorkspaceRoleOverride {
            codex: Some(AgentAuthConfig {
                sync_source_dir: Some(role_source.clone()),
                ..Default::default()
            }),
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
    assert_eq!(
        reloaded
            .claude
            .as_ref()
            .and_then(|c| c.sync_source_dir.clone()),
        Some(workspace_source)
    );
    assert_eq!(
        reloaded
            .roles
            .get("smith")
            .and_then(|r| r.codex.as_ref())
            .and_then(|c| c.sync_source_dir.clone()),
        Some(role_source)
    );

    let mut cleared = reloaded.clone();
    cleared.claude = None;
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
    assert!(workspace.claude.is_none());
    assert!(workspace.roles.is_empty());

    let out = workspace_file_contents(&paths, "proj");
    assert!(!out.contains("sync_source_dir"), "{out}");
}
