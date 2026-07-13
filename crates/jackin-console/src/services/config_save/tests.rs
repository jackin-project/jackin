use std::collections::BTreeMap;
use std::path::PathBuf;

use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, EnvScope, EnvValue, GithubAuthConfig,
    GithubAuthMode, KeepAwakeConfig, MountConfig, MountIsolation, WorkspaceConfig,
    WorkspaceRoleOverride,
};
use jackin_core::{Agent, WorkspaceName};

fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

use super::{
    EditorSavePreviewError, EditorSavePreviewInput, EditorSavePreviewPlan, WorkspaceSaveDiffOp,
    build_workspace_edit, plan_editor_save_preview, pre_existing_redundant_mounts_message,
    workspace_save_diff_plan,
};
use crate::services::config_save::validate_settings_env;
use crate::tui::screens::settings::model::{SettingsEnvConfig, SettingsTrustRow};

fn mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}

#[test]
fn workspace_save_diff_plan_captures_auth_and_source_dir_changes() {
    let original = WorkspaceConfig::default();
    let mut pending = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            sync_source_dir: Some(PathBuf::from("/host/claude")),
        }),
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            env: BTreeMap::default(),
        }),
        ..Default::default()
    };
    pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            codex: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::OAuthToken,
                sync_source_dir: Some(PathBuf::from("/host/codex")),
            }),
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Ignore,
                env: BTreeMap::default(),
            }),
            ..Default::default()
        },
    );

    let ops = workspace_save_diff_plan(&wn("proj"), &original, &pending);

    assert!(ops.contains(&WorkspaceSaveDiffOp::WorkspaceAuthForward {
        agent: Agent::Claude,
        mode: Some(AuthForwardMode::ApiKey),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::WorkspaceSyncSourceDir {
        agent: Agent::Claude,
        source: Some(PathBuf::from("/host/claude")),
    }));
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceGithubAuthForward {
            mode: Some(GithubAuthMode::Token),
        })
    );
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceRoleAuthForward {
            role: "smith".into(),
            agent: Agent::Codex,
            mode: Some(AuthForwardMode::OAuthToken),
        })
    );
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceRoleSyncSourceDir {
            role: "smith".into(),
            agent: Agent::Codex,
            source: Some(PathBuf::from("/host/codex")),
        })
    );
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceRoleGithubAuthForward {
            role: "smith".into(),
            mode: Some(GithubAuthMode::Ignore),
        })
    );
}

#[test]
fn workspace_save_diff_plan_captures_env_set_and_remove_for_layers() {
    let mut original = WorkspaceConfig::default();
    original
        .env
        .insert("OLD".into(), EnvValue::Plain("remove".into()));
    original
        .env
        .insert("KEEP".into(), EnvValue::Plain("same".into()));
    original.github = Some(GithubAuthConfig {
        env: [("GH_OLD".into(), EnvValue::Plain("remove".into()))].into(),
        ..Default::default()
    });
    original.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            env: [("ROLE_OLD".into(), EnvValue::Plain("remove".into()))].into(),
            github: Some(GithubAuthConfig {
                env: [("ROLE_GH_OLD".into(), EnvValue::Plain("remove".into()))].into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut pending = WorkspaceConfig::default();
    pending
        .env
        .insert("KEEP".into(), EnvValue::Plain("same".into()));
    pending
        .env
        .insert("NEW".into(), EnvValue::Plain("set".into()));
    pending.github = Some(GithubAuthConfig {
        env: [("GH_NEW".into(), EnvValue::Plain("set".into()))].into(),
        ..Default::default()
    });
    pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            env: [("ROLE_NEW".into(), EnvValue::Plain("set".into()))].into(),
            github: Some(GithubAuthConfig {
                env: [("ROLE_GH_NEW".into(), EnvValue::Plain("set".into()))].into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let ops = workspace_save_diff_plan(&wn("proj"), &original, &pending);

    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvSet {
        scope: EnvScope::Workspace("proj".into()),
        key: "NEW".into(),
        value: EnvValue::Plain("set".into()),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvRemove {
        scope: EnvScope::Workspace("proj".into()),
        key: "OLD".into(),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvSet {
        scope: EnvScope::WorkspaceGithub("proj".into()),
        key: "GH_NEW".into(),
        value: EnvValue::Plain("set".into()),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvRemove {
        scope: EnvScope::WorkspaceGithub("proj".into()),
        key: "GH_OLD".into(),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvSet {
        scope: EnvScope::WorkspaceRole {
            workspace: "proj".into(),
            role: "smith".into(),
        },
        key: "ROLE_NEW".into(),
        value: EnvValue::Plain("set".into()),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvRemove {
        scope: EnvScope::WorkspaceRoleGithub {
            workspace: "proj".into(),
            role: "smith".into(),
        },
        key: "ROLE_GH_OLD".into(),
    }));
}

#[test]
fn build_workspace_edit_emits_keep_awake_change_only_when_diffed() {
    let original = WorkspaceConfig {
        workdir: "/workspace/proj".into(),
        mounts: vec![mount("/work", "/workspace/proj")],
        keep_awake: KeepAwakeConfig { enabled: false },
        ..Default::default()
    };

    let pending_unchanged = original.clone();
    let edit = build_workspace_edit(&original, &pending_unchanged);
    assert_eq!(edit.keep_awake_enabled, None);

    let pending_on = WorkspaceConfig {
        keep_awake: KeepAwakeConfig { enabled: true },
        ..original.clone()
    };
    let edit = build_workspace_edit(&original, &pending_on);
    assert_eq!(edit.keep_awake_enabled, Some(true));

    let original_on = WorkspaceConfig {
        keep_awake: KeepAwakeConfig { enabled: true },
        ..original.clone()
    };
    let pending_off = WorkspaceConfig {
        keep_awake: KeepAwakeConfig { enabled: false },
        ..original
    };
    let edit = build_workspace_edit(&original_on, &pending_off);
    assert_eq!(edit.keep_awake_enabled, Some(false));
}

#[test]
fn plan_editor_save_preview_reports_missing_create_name() {
    let pending = WorkspaceConfig::default();
    let error = plan_editor_save_preview(
        &AppConfig::default(),
        EditorSavePreviewInput::Create {
            pending: &pending,
            pending_name: None,
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, EditorSavePreviewError::Message(message) if message == "missing workspace name")
    );
}

#[test]
fn plan_editor_save_preview_plans_edit_removals() {
    let original = WorkspaceConfig {
        workdir: "/workspace/proj".into(),
        mounts: vec![mount("/old", "/workspace/proj"), mount("/data", "/data")],
        ..Default::default()
    };
    let pending = WorkspaceConfig {
        mounts: vec![mount("/new", "/workspace/proj")],
        ..original.clone()
    };
    let mut config = AppConfig::default();
    config.workspaces.insert("proj".into(), original.clone());

    let plan = plan_editor_save_preview(
        &config,
        EditorSavePreviewInput::Edit {
            original_name: "proj",
            original: &original,
            pending: &pending,
        },
    )
    .unwrap();

    let EditorSavePreviewPlan::Edit {
        effective_removals,
        edit_driven_collapses,
    } = plan
    else {
        panic!("expected edit preview plan");
    };
    assert_eq!(effective_removals, vec!["/data".to_owned()]);
    assert!(edit_driven_collapses.is_empty());
}

#[test]
fn pre_existing_redundant_mounts_message_names_workspace_and_paths() {
    let parent = mount("/home/user/project", "/workspace");
    let child = mount("/home/user/project/sub", "/workspace/sub");
    let message = pre_existing_redundant_mounts_message(
        "proj",
        &[jackin_config::Removal {
            child,
            covered_by: parent,
        }],
    );

    assert!(message.contains("pre-existing redundant mount(s) in this workspace"));
    assert!(message.contains("run `jackin❯ workspace prune proj`"));
}

#[test]
fn validate_settings_env_accepts_registered_roles_and_regular_keys() {
    let env = SettingsEnvConfig {
        env: [("PROJECT_ENV".to_owned(), "value")].into(),
        roles: [(
            "smith".to_owned(),
            [("ROLE_ENV".to_owned(), "value")].into(),
        )]
        .into(),
    };
    let roles = vec![SettingsTrustRow {
        role: "smith".into(),
        git: "builtin".into(),
        trusted: true,
    }];

    validate_settings_env(&env, &roles).unwrap();
}

#[test]
fn validate_settings_env_rejects_unregistered_role_keys() {
    let env = SettingsEnvConfig {
        env: BTreeMap::default(),
        roles: [(
            "unknown".to_owned(),
            [("ROLE_ENV".to_owned(), "value")].into(),
        )]
        .into(),
    };

    let error = validate_settings_env(&env, &[]).unwrap_err().to_string();

    assert!(error.contains("role \"unknown\" is not registered"));
}

#[test]
fn validate_settings_env_rejects_empty_and_reserved_keys() {
    let empty = SettingsEnvConfig {
        env: [(" ".to_owned(), "value")].into(),
        roles: BTreeMap::default(),
    };
    assert!(
        validate_settings_env(&empty, &[])
            .unwrap_err()
            .to_string()
            .contains("env var key cannot be empty")
    );

    let reserved = SettingsEnvConfig {
        env: [("JACKIN_WORKDIR".to_owned(), "value")].into(),
        roles: BTreeMap::default(),
    };
    assert!(
        validate_settings_env(&reserved, &[])
            .unwrap_err()
            .to_string()
            .contains("is reserved by the jackin runtime")
    );
}
