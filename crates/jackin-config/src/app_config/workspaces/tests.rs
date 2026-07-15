// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `workspaces`.
use super::*;
use crate::MountConfig;
use crate::{CURRENT_WORKSPACE_VERSION, KeepAwakeConfig};
use jackin_core::WorkspaceName;
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
use tempfile::tempdir;

#[test]
fn edit_workspace_leaves_original_value_when_validation_fails() {
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/project".to_owned(),
        mounts: vec![MountConfig {
            src: temp.path().display().to_string(),
            dst: "/workspace/project".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        allowed_roles: vec!["agent-smith".to_owned()],
        default_role: Some("agent-smith".to_owned()),
        default_agent: None,
        last_role: None,
        env: std::collections::BTreeMap::new(),
        roles: std::collections::BTreeMap::new(),
        keep_awake: KeepAwakeConfig::default(),
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
        runtime: crate::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
    };
    config
        .create_workspace(
            &WorkspaceName::parse("big-monorepo").unwrap(),
            original.clone(),
        )
        .unwrap();

    let err = config
        .edit_workspace(
            &wn("big-monorepo"),
            WorkspaceEdit {
                workdir: Some("/workspace/elsewhere".to_owned()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn edit_workspace_toggles_keep_awake_when_set() {
    // Round-trip: enable, disable, no-change. The Option<bool> shape
    // distinguishes "user touched the field" from "user said nothing
    // about it", which is the whole point of the field type.
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("my-app").unwrap(),
            WorkspaceConfig {
                version: CURRENT_WORKSPACE_VERSION.to_owned(),
                workdir: "/workspace/proj".to_owned(),
                mounts: vec![MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace/proj".to_owned(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!config.workspaces.get("my-app").unwrap().keep_awake.enabled);

    config
        .edit_workspace(
            &wn("my-app"),
            WorkspaceEdit {
                keep_awake_enabled: Some(true),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert!(config.workspaces.get("my-app").unwrap().keep_awake.enabled);

    // Subsequent edit with no keep_awake change must leave the
    // field alone — this is the contract that lets `workspace edit
    // --workdir` not silently flip power-management state.
    config
        .edit_workspace(
            &wn("my-app"),
            WorkspaceEdit {
                workdir: Some("/workspace/proj".to_owned()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert!(
        config.workspaces.get("my-app").unwrap().keep_awake.enabled,
        "unrelated edits must not flip keep_awake",
    );

    config
        .edit_workspace(
            &wn("my-app"),
            WorkspaceEdit {
                keep_awake_enabled: Some(false),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert!(!config.workspaces.get("my-app").unwrap().keep_awake.enabled);
}

#[test]
fn edit_workspace_sets_and_clears_agent() {
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("my-app").unwrap(),
            WorkspaceConfig {
                version: CURRENT_WORKSPACE_VERSION.to_owned(),
                workdir: "/workspace/proj".to_owned(),
                mounts: vec![MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace/proj".to_owned(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();

    config
        .edit_workspace(
            &wn("my-app"),
            WorkspaceEdit {
                default_agent: Some(Some(jackin_core::Agent::Codex)),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert_eq!(
        config.workspaces.get("my-app").unwrap().default_agent,
        Some(jackin_core::Agent::Codex)
    );

    config
        .edit_workspace(
            &wn("my-app"),
            WorkspaceEdit {
                workdir: Some("/workspace/proj".to_owned()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert_eq!(
        config.workspaces.get("my-app").unwrap().default_agent,
        Some(jackin_core::Agent::Codex),
        "unrelated edits must not clear default_agent"
    );

    config
        .edit_workspace(
            &wn("my-app"),
            WorkspaceEdit {
                default_agent: Some(None),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert_eq!(config.workspaces.get("my-app").unwrap().default_agent, None);
}

#[test]
fn create_workspace_rejects_duplicate_name_and_preserves_existing_value() {
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/project".to_owned(),
        mounts: vec![MountConfig {
            src: temp.path().display().to_string(),
            dst: "/workspace/project".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    config
        .create_workspace(
            &WorkspaceName::parse("big-monorepo").unwrap(),
            original.clone(),
        )
        .unwrap();

    let err = config
        .create_workspace(
            &WorkspaceName::parse("big-monorepo").unwrap(),
            WorkspaceConfig {
                version: CURRENT_WORKSPACE_VERSION.to_owned(),
                workdir: "/workspace/other".to_owned(),
                mounts: vec![MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace/other".to_owned(),
                    readonly: true,
                    isolation: crate::MountIsolation::Shared,
                }],
                allowed_roles: vec!["agent-smith".to_owned()],
                default_role: Some("agent-smith".to_owned()),
                ..Default::default()
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains("already exists"));
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn edit_workspace_rejects_duplicate_upsert_destinations() {
    let temp = tempdir().unwrap();
    let original_src = temp.path().join("project");
    let first_upsert = temp.path().join("cache-a");
    let second_upsert = temp.path().join("cache-b");
    std::fs::create_dir_all(&original_src).unwrap();
    std::fs::create_dir_all(&first_upsert).unwrap();
    std::fs::create_dir_all(&second_upsert).unwrap();

    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/project".to_owned(),
        mounts: vec![MountConfig {
            src: original_src.display().to_string(),
            dst: "/workspace/project".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    config
        .create_workspace(
            &WorkspaceName::parse("big-monorepo").unwrap(),
            original.clone(),
        )
        .unwrap();

    let err = config
        .edit_workspace(
            &wn("big-monorepo"),
            WorkspaceEdit {
                upsert_mounts: vec![
                    MountConfig {
                        src: first_upsert.display().to_string(),
                        dst: "/workspace/cache".to_owned(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                    MountConfig {
                        src: second_upsert.display().to_string(),
                        dst: "/workspace/cache".to_owned(),
                        readonly: true,
                        isolation: crate::MountIsolation::Shared,
                    },
                ],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("duplicate workspace edit mount destination")
    );
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn edit_workspace_rejects_missing_remove_destination() {
    let temp = tempdir().unwrap();
    let original_src = temp.path().join("project");
    std::fs::create_dir_all(&original_src).unwrap();

    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/project".to_owned(),
        mounts: vec![MountConfig {
            src: original_src.display().to_string(),
            dst: "/workspace/project".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    config
        .create_workspace(
            &WorkspaceName::parse("big-monorepo").unwrap(),
            original.clone(),
        )
        .unwrap();

    let err = config
        .edit_workspace(
            &wn("big-monorepo"),
            WorkspaceEdit {
                remove_destinations: vec!["/workspace/missing".to_owned()],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("unknown workspace mount destination")
    );
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn remove_workspace_errors_when_missing() {
    let mut config = AppConfig::default();

    let err = config.remove_workspace(&wn("missing")).unwrap_err();

    assert!(matches!(err, ConfigError::UnknownWorkspace(name) if name == "missing"));
}
