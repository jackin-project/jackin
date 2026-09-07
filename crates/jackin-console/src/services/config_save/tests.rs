// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_config::{
    AppConfig, EnvScope, EnvValue, GithubAuthConfig, KeepAwakeConfig, MountConfig, MountIsolation,
    WorkspaceConfig, WorkspaceRoleOverride,
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
fn workspace_save_diff_plan_captures_account_assignments_and_bindings() {
    let original = WorkspaceConfig::default();
    let pending = WorkspaceConfig {
        accounts: vec!["work".into(), "personal".into()],
        account_bindings: [(Agent::Claude, "work".into())].into(),
        roles: [(
            "smith".into(),
            WorkspaceRoleOverride {
                account_bindings: [(Agent::Claude, "personal".into())].into(),
                ..Default::default()
            },
        )]
        .into(),
        ..Default::default()
    };
    let ops = workspace_save_diff_plan(&wn("proj"), &original, &pending);
    assert_eq!(
        ops,
        vec![
            WorkspaceSaveDiffOp::WorkspaceAccounts {
                accounts: pending.accounts.clone()
            },
            WorkspaceSaveDiffOp::WorkspaceAccountBinding {
                agent: Agent::Claude,
                account: Some("work".into())
            },
            WorkspaceSaveDiffOp::WorkspaceRoleAccountBinding {
                role: "smith".into(),
                agent: Agent::Claude,
                account: Some("personal".into())
            },
        ]
    );
    let removed = workspace_save_diff_plan(&wn("proj"), &pending, &original);
    assert!(
        removed.contains(&WorkspaceSaveDiffOp::WorkspaceAccountBinding {
            agent: Agent::Claude,
            account: None
        })
    );
    assert!(
        removed.contains(&WorkspaceSaveDiffOp::WorkspaceRoleAccountBinding {
            role: "smith".into(),
            agent: Agent::Claude,
            account: None
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

#[test]
fn settings_save_round_trips_multiple_accounts_and_github_independently() {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let config = jackin_config::ConfigEditor::open(&paths)
        .unwrap()
        .save()
        .unwrap();
    let mut settings = crate::tui::state::SettingsState::from_config(&config);
    for id in ["personal", "work"] {
        settings.auth.pending.insert(
            id.into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: id.into(),
                provider: jackin_config::AiProvider::OpenAi,
                credential: jackin_config::AccountCredential::ApiKey {
                    value: EnvValue::Plain(format!("secret-{id}")),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    settings.auth.github.auth_forward = jackin_config::GithubAuthMode::Token;
    settings
        .auth
        .github
        .env
        .insert("GH_TOKEN".into(), EnvValue::Plain("github-secret".into()));
    let save = |settings: &crate::tui::state::SettingsState<'_>| {
        super::save_settings(
            &paths,
            super::SettingsSaveInput {
                mounts_original: &settings.mounts.original,
                mounts_pending: &settings.mounts.pending,
                env_original: &settings.env.original,
                env_pending: &settings.env.pending,
                auth_original: &settings.auth.original,
                auth_pending: &settings.auth.pending,
                github: &settings.auth.github,
                original_github: &settings.auth.original_github,
                bindings_pending: &settings.auth.bindings,
                bindings_original: &settings.auth.original_bindings,
                trust_pending: &settings.trust.pending,
                git_coauthor_trailer: settings.general.pending_coauthor_trailer,
                git_dco: settings.general.pending_dco,
            },
        )
        .unwrap()
    };
    let saved = save(&settings);
    assert_eq!(saved.accounts, settings.auth.pending);
    assert_eq!(saved.github.as_ref(), Some(&settings.auth.github));
    settings.mark_saved();
    settings.auth.pending.remove("work");
    settings.auth.pending.get_mut("personal").unwrap().name = "Renamed personal".into();
    let saved = save(&settings);
    assert!(!saved.accounts.contains_key("work"));
    assert_eq!(saved.accounts["personal"].name, "Renamed personal");
    assert_eq!(saved.github.as_ref(), Some(&settings.auth.github));
}
