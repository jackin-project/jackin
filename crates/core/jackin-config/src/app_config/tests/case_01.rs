// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn deserializes_scoped_docker_mounts() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "~/.chainargos/brown", dst = "/config" }
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let mounts = &config.docker.mounts;
    match mounts.get("chainargos/*").unwrap() {
        MountEntry::Scoped(scope) => {
            let m = scope.get("chainargos-secrets").unwrap();
            assert_eq!(m.dst, "/secrets");
            assert!(m.readonly);
        }
        MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
    }
    match mounts.get("chainargos/agent-brown").unwrap() {
        MountEntry::Scoped(scope) => {
            let m = scope.get("brown-config").unwrap();
            assert_eq!(m.dst, "/config");
            assert!(!m.readonly);
        }
        MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
    }
}

#[test]
fn deserializes_saved_workspaces() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/Users/donbeave/Projects/chainargos/big-monorepo"
default_role = "agent-smith"
allowed_roles = ["agent-smith", "chainargos/the-architect"]

[[workspaces.big-monorepo.mounts]]
src = "/Users/donbeave/Projects/chainargos/big-monorepo"
dst = "/Users/donbeave/Projects/chainargos/big-monorepo"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/cache"
dst = "/workspace/cache"
readonly = true
"#;

    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let workspace = config.workspaces.get("big-monorepo").unwrap();

    assert_eq!(
        workspace.workdir,
        "/Users/donbeave/Projects/chainargos/big-monorepo"
    );
    assert_eq!(workspace.mounts.len(), 2);
    assert_eq!(workspace.default_role.as_deref(), Some("agent-smith"));
    assert_eq!(workspace.allowed_roles.len(), 2);
    assert!(workspace.mounts[1].readonly);
}

#[test]
fn deserializes_global_telemetry_config() {
    let toml_str = r#"
[telemetry]
level = "trace"
categories = ["docker", "launch"]
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();

    assert_eq!(
        config.telemetry.level,
        Some(crate::TelemetryLevelConfig::Trace)
    );
    assert_eq!(config.telemetry.categories, vec!["docker", "launch"]);
}

#[test]
fn default_telemetry_config_is_not_serialized() {
    let config = AppConfig::default();
    let toml = toml::to_string_pretty(&config).unwrap();

    assert!(!toml.contains("[telemetry]"), "{toml}");
}

#[test]
fn rejects_workspace_with_workdir_outside_mounts() {
    let temp = tempdir().unwrap();

    let workspace = WorkspaceConfig {
        workdir: "/workspace/project".to_owned(),
        mounts: vec![MountConfig {
            src: temp.path().display().to_string(),
            dst: "/workspace/src".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    };

    let error =
        validate_workspace_config(&WorkspaceName::parse("big-monorepo").unwrap(), &workspace)
            .unwrap_err();

    assert!(error.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn edit_workspace_does_not_persist_invalid_mutation() {
    use crate::WorkspaceEdit;
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    let src = temp.path().display().to_string();

    config
        .create_workspace(
            &WorkspaceName::parse("big-monorepo").unwrap(),
            WorkspaceConfig {
                workdir: "/workspace/project".to_owned(),
                mounts: vec![MountConfig {
                    src,
                    dst: "/workspace/project".to_owned(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();

    let error = config
        .edit_workspace(
            &wn("big-monorepo"),
            WorkspaceEdit {
                workdir: Some("/workspace/missing".to_owned()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(error.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
    assert_eq!(
        config.workspaces.get("big-monorepo").unwrap().workdir,
        "/workspace/project"
    );
}

#[test]
fn load_or_init_rejects_invalid_saved_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp"
dst = "/workspace/src"
"#,
    )
    .unwrap();

    let error = AppConfig::load_or_init(&paths).unwrap_err();

    assert!(error.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn load_or_init_rejects_invalid_persisted_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mount_src = temp.path().join("workspace-src");
    std::fs::create_dir_all(&mount_src).unwrap();

    let toml_str = format!(
        r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.broken]
workdir = "/workspace/project"

[[workspaces.broken.mounts]]
src = "{}"
dst = "/workspace/src"
"#,
        mount_src.display()
    );

    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, toml_str).unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    assert!(err.to_string().contains("workspace \"broken\" workdir must be equal to, inside, or a parent of one of the workspace mount destinations"));
}

#[test]
fn edit_workspace_rejects_upsert_that_introduces_child_under_existing_parent() {
    use crate::{MountConfig, WorkspaceConfig, WorkspaceEdit};

    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a".into(),
                mounts: vec![MountConfig {
                    src: "/a".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();

    let err = config
        .edit_workspace(
            &wn("test"),
            WorkspaceEdit {
                upsert_mounts: vec![MountConfig {
                    src: "/a/b".into(),
                    dst: "/a/b".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("already covered") || msg.contains("redundant"),
        "expected 'already covered' or 'redundant' in error message, got: {msg}"
    );
}

#[test]
fn edit_workspace_rejects_upsert_with_readonly_mismatch_vs_existing_child() {
    use crate::{MountConfig, WorkspaceConfig, WorkspaceEdit};

    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a/b".into(),
                mounts: vec![MountConfig {
                    src: "/a/b".into(),
                    dst: "/a/b".into(),
                    readonly: true,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();

    let err = config
        .edit_workspace(
            &wn("test"),
            WorkspaceEdit {
                upsert_mounts: vec![MountConfig {
                    src: "/a".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains("readonly"));
}

#[test]
fn edit_workspace_accepts_pre_collapsed_upsert_that_replaces_children() {
    // CLI's job is to pre-collapse. Here we simulate it: instead of
    // upserting just the parent (which would leave children as redundants
    // and fail the post-condition), the CLI removes the children via
    // remove_destinations AND upserts the parent in the same edit.
    use crate::{MountConfig, WorkspaceConfig, WorkspaceEdit};

    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a/b".into(),
                mounts: vec![
                    MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                    MountConfig {
                        src: "/a/c".into(),
                        dst: "/a/c".into(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap();

    config
        .edit_workspace(
            &wn("test"),
            WorkspaceEdit {
                upsert_mounts: vec![MountConfig {
                    src: "/a".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                remove_destinations: vec!["/a/b".into(), "/a/c".into()],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();

    let ws = config
        .list_workspaces()
        .into_iter()
        .find(|(n, _)| *n == "test")
        .map(|(_, w)| w)
        .expect("workspace should exist");
    assert_eq!(ws.mounts.len(), 1);
    assert_eq!(ws.mounts[0].src, "/a");
}
