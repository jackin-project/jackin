// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::Effect;
use jackin_console::domain::fixed::Fixed;
use jackin_console::domain::vocabulary::fill_in;
use jackin_core::{
    AccountName, AllowedRoles, ConfigEdit, EnvName, EnvScope, HostPath, PlainEnvValue, RoleName,
    RoleSet, Workspace, WorkspaceName,
};
use jackin_testkit::{TestResult, guard};

use super::support::Lab;

type Out = Result<(), Box<dyn std::error::Error>>;

const NEW: &str = "n down enter";

fn value(text: &str) -> Result<PlainEnvValue, Box<dyn std::error::Error>> {
    Ok(PlainEnvValue::parse(text)?)
}

fn set(scope: EnvScope, name: &str, text: &str) -> Result<ConfigEdit, Box<dyn std::error::Error>> {
    Ok(ConfigEdit::SetEnv {
        scope,
        name: EnvName::parse(name)?,
        value: Some(value(text)?),
    })
}

fn name(text: &str) -> Result<WorkspaceName, Box<dyn std::error::Error>> {
    Ok(WorkspaceName::parse(text)?)
}

async fn second_workspace(lab: &Lab) -> Out {
    let path = HostPath::parse(&std::fs::canonicalize(lab.code.join("alpha"))?)?;
    lab.edit(vec![ConfigEdit::AddWorkspace {
        name: name("second")?,
        workspace: Workspace::new(path, false),
    }])
    .await?;
    Ok(())
}

#[tokio::test]
async fn create_from_the_wizard_saves_the_canonical_path() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.console.keys(NEW)?;
        lab.console.keys("down space")?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("Project folder /sandbox/project"),
            "{screen}"
        );
        lab.console.keys("enter")?;
        let screen = lab.console.screen();
        assert!(screen.contains("step 4 of 5"), "{screen}");
        lab.console.keys("enter enter")?;
        lab.save().await?;
        lab.console
            .keys("n down enter down down space enter enter enter")?;
        lab.save().await?;
        let config = lab.config().await?;
        for folder in ["alpha", "beta"] {
            let saved = config.workspace(&name(folder)?).ok_or(folder)?;
            let expected = HostPath::parse(&std::fs::canonicalize(lab.code.join(folder))?)?;
            assert_eq!(saved.path, expected);
            assert!(!saved.docker);
        }
        assert_eq!(lab.console.status_line(), "Workspace beta saved");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_taken_wizard_name_keeps_the_input() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let path = HostPath::parse(&std::fs::canonicalize(lab.code.join("beta"))?)?;
        lab.edit(vec![ConfigEdit::AddWorkspace {
            name: name("alpha")?,
            workspace: Workspace::new(path, false),
        }])
        .await?;
        lab.reopen().await?;
        let before = lab.console.config_text().await?;
        lab.console.keys(NEW)?;
        lab.console.keys("down space enter enter enter")?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("A workspace named alpha already exists") && screen.contains("alpha"),
            "{screen}"
        );
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_name_taken_by_another_handle_is_refused_with_the_core_message() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.console.keys("down e enter")?;
        lab.console.type_text("second");
        lab.console.keys("enter")?;
        second_workspace(&lab).await?;
        let before = lab.console.config_text().await?;
        lab.save().await?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("duplicate workspace name second")
                && screen.contains("your edits are intact"),
            "{screen}"
        );
        assert_eq!(lab.console.config_text().await?, before);
        lab.console.keys("esc")?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("second") && screen.contains("1 change"),
            "{screen}"
        );
        Ok(())
    })
    .await
}

async fn rich(lab: &Lab) -> Out {
    lab.add_role("other").await?;
    let work = AccountName::parse("work")?;
    let sentinel = RoleName::parse("sentinel")?;
    let origin = name("platform-backend")?;
    lab.edit(vec![
        ConfigEdit::SetWorkspaceDocker {
            workspace: origin.clone(),
            docker: true,
        },
        ConfigEdit::SetWorkspaceRoles {
            workspace: origin.clone(),
            roles: AllowedRoles::Only(
                RoleSet::new([sentinel.clone(), RoleName::parse("other")?]).ok_or("roles")?,
            ),
        },
        ConfigEdit::SetWorkspaceDefaultRole {
            workspace: origin.clone(),
            role: Some(sentinel.clone()),
        },
        set(EnvScope::Workspace(origin.clone()), "ALPHA", "1")?,
        set(
            EnvScope::WorkspaceRole(origin.clone(), sentinel),
            "BETA",
            "2",
        )?,
        ConfigEdit::BindWorkspaceAccount {
            workspace: origin,
            account: work,
        },
    ])
    .await?;
    Ok(())
}

#[tokio::test]
async fn rename_copies_from_the_saved_entry() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        rich(&lab).await?;
        lab.reopen().await?;
        lab.console.keys("down e enter")?;
        lab.console.type_text("renamed");
        lab.console.keys("enter")?;
        lab.edit(vec![set(
            EnvScope::Workspace(name("platform-backend")?),
            "GAMMA",
            "3",
        )?])
        .await?;
        lab.save().await?;
        let config = lab.config().await?;
        assert!(config.workspace(&name("platform-backend")?).is_none());
        let moved = config.workspace(&name("renamed")?).ok_or("renamed")?;
        assert!(moved.docker);
        assert_eq!(moved.default_role, Some(RoleName::parse("sentinel")?));
        assert!(matches!(&moved.roles, AllowedRoles::Only(set) if set.iter().count() == 2));
        let keys: Vec<&str> = moved
            .env
            .keys()
            .map(jackin_core::ScopedEnvName::as_str)
            .collect();
        assert_eq!(keys, ["ALPHA", "GAMMA"]);
        let scoped = moved
            .role_env
            .get(&RoleName::parse("sentinel")?)
            .ok_or("role env")?;
        assert_eq!(
            scoped
                .keys()
                .map(jackin_core::ScopedEnvName::as_str)
                .collect::<Vec<_>>(),
            ["BETA"]
        );
        assert_eq!(
            moved
                .bindings
                .values()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["work"]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn delete_removes_the_key_and_keeps_the_sandbox() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        second_workspace(&lab).await?;
        lab.rig.launch(1).await?;
        lab.reopen().await?;
        lab.console.keys("down down d right enter")?;
        lab.console.settle().await?;
        let config = lab.config().await?;
        assert!(config.workspace(&name("second")?).is_none());
        assert!(config.workspace(&name("platform-backend")?).is_some());
        assert_eq!(lab.rig.world.gateway.sandboxes().len(), 1);
        assert_eq!(lab.world().instances.len(), 1);
        let status = lab.console.status_line();
        assert!(status.contains("Deleted workspace second"), "{status}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn persist_diffs_the_payload_against_its_base() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        lab.reopen().await?;
        lab.console.keys("down e")?;
        lab.edit(vec![ConfigEdit::SetWorkspaceDocker {
            workspace: name("platform-backend")?,
            docker: true,
        }])
        .await?;
        lab.console.keys("3 down enter")?;
        lab.save().await?;
        let saved = lab.config().await?;
        let saved = saved
            .workspace(&name("platform-backend")?)
            .ok_or("workspace")?;
        assert_eq!(saved.default_role, Some(RoleName::parse("other")?));
        assert!(saved.docker, "the other handle's docker was written back");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn both_changed_the_editor_wins() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        lab.reopen().await?;
        lab.console.keys("down e")?;
        lab.edit(vec![
            ConfigEdit::SetWorkspaceDocker {
                workspace: name("platform-backend")?,
                docker: true,
            },
            ConfigEdit::SetWorkspaceDefaultRole {
                workspace: name("platform-backend")?,
                role: Some(RoleName::parse("sentinel")?),
            },
        ])
        .await?;
        lab.console.keys("3 down enter")?;
        lab.save().await?;
        let saved = lab.config().await?;
        let saved = saved
            .workspace(&name("platform-backend")?)
            .ok_or("workspace")?;
        assert_eq!(saved.default_role, Some(RoleName::parse("other")?));
        assert!(saved.docker);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn collections_diff_per_key() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let origin = name("platform-backend")?;
        lab.edit(vec![set(
            EnvScope::Workspace(origin.clone()),
            "KEPT_C",
            "c",
        )?])
        .await?;
        lab.reopen().await?;
        let base = lab.workspace("platform-backend").ok_or("workspace")?;
        lab.edit(vec![set(
            EnvScope::Workspace(origin.clone()),
            "OTHER_B",
            "b",
        )?])
        .await?;
        let mut payload = base.clone();
        payload.env.retain(|v| v.key != "KEPT_C");
        payload
            .env
            .push(jackin_console::domain::workspace::EnvVar::plain(
                "EDITOR_A", "a",
            ));
        lab.perform(Effect::SaveWorkspace {
            id: Some(base.id),
            base: Some(Box::new(base)),
            workspace: Box::new(payload),
        })
        .await?;
        let saved = lab.config().await?;
        let keys: Vec<&str> = saved
            .workspace(&origin)
            .ok_or("workspace")?
            .env
            .keys()
            .map(jackin_core::ScopedEnvName::as_str)
            .collect();
        assert_eq!(keys, ["EDITOR_A", "OTHER_B"]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn global_env_merges_per_key() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.edit(vec![set(EnvScope::Global, "YES", "1")?]).await?;
        lab.reopen().await?;
        let base = lab.world().global.clone();
        lab.edit(vec![set(EnvScope::Global, "OTHER_X", "x")?])
            .await?;
        let mut payload = base.clone();
        payload.env = vec![jackin_console::domain::workspace::EnvVar::plain("YES", "2")];
        lab.perform(Effect::SaveGlobal {
            base: Box::new(base),
            global: Box::new(payload),
        })
        .await?;
        let saved = lab.config().await?;
        let env: Vec<(String, String)> = saved
            .env()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(
            env,
            [
                ("OTHER_X".to_owned(), "x".to_owned()),
                ("YES".to_owned(), "2".to_owned())
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unrepresentable_payload_writes_nothing() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.console.config_text().await?;
        let base = lab.workspace("platform-backend").ok_or("workspace")?;
        let mut payload = base.clone();
        payload.keep_awake = true;
        lab.perform(Effect::SaveWorkspace {
            id: Some(base.id),
            base: Some(Box::new(base)),
            workspace: Box::new(payload),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_removed_target_fails_visibly_and_the_world_reloads() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.console.keys("down e enter")?;
        lab.console.type_text("renamed");
        lab.console.keys("enter")?;
        lab.edit(vec![ConfigEdit::RemoveWorkspace(name("platform-backend")?)])
            .await?;
        lab.save().await?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("unknown workspace platform-backend")
                && screen.contains("your edits are intact"),
            "{screen}"
        );
        assert!(lab.config().await?.workspace(&name("renamed")?).is_none());
        assert!(lab.world().workspaces.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cursor_survives_a_workspace_insert() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.console.keys("down")?;
        let path = HostPath::parse(&std::fs::canonicalize(lab.code.join("beta"))?)?;
        lab.edit(vec![ConfigEdit::AddWorkspace {
            name: name("aaa")?,
            workspace: Workspace::new(path, false),
        }])
        .await?;
        for _ in 0..8 {
            lab.console.elapse(1);
            lab.console.tick(1);
        }
        lab.console.settle().await?;
        let screen = lab.console.screen();
        assert!(screen.contains("aaa"), "{screen}");
        let cursor = screen
            .lines()
            .find(|line| line.contains('▎'))
            .ok_or("no cursor")?;
        assert!(cursor.contains("platform-backend"), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn allowed_roles_reach_the_config() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        lab.edit(vec![ConfigEdit::SetWorkspaceDefaultRole {
            workspace: name("platform-backend")?,
            role: Some(RoleName::parse("sentinel")?),
        }])
        .await?;
        lab.reopen().await?;
        lab.console.keys("down e 3 down space")?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("allowed 1 of 2") || screen.contains("Allowed roles"),
            "{screen}"
        );
        lab.save().await?;
        let config = lab.config().await?;
        let saved = config
            .workspace(&name("platform-backend")?)
            .ok_or("workspace")?;
        let only: Vec<String> = match &saved.roles {
            AllowedRoles::Only(set) => set.iter().map(ToString::to_string).collect(),
            AllowedRoles::All => Vec::new(),
        };
        assert_eq!(only, ["sentinel"]);
        assert_eq!(saved.default_role, Some(RoleName::parse("sentinel")?));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unchecking_the_default_clears_it_in_the_same_write() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        lab.edit(vec![ConfigEdit::SetWorkspaceDefaultRole {
            workspace: name("platform-backend")?,
            role: Some(RoleName::parse("sentinel")?),
        }])
        .await?;
        lab.reopen().await?;
        lab.console.keys("down e 3 down down space")?;
        lab.save().await?;
        let config = lab.config().await?;
        let saved = config
            .workspace(&name("platform-backend")?)
            .ok_or("workspace")?;
        let only: Vec<String> = match &saved.roles {
            AllowedRoles::Only(set) => set.iter().map(ToString::to_string).collect(),
            AllowedRoles::All => Vec::new(),
        };
        assert_eq!(only, ["other"]);
        assert_eq!(saved.default_role, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_variable_moves_between_scopes() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let origin = name("platform-backend")?;
        lab.edit(vec![set(
            EnvScope::Workspace(origin.clone()),
            "MODE",
            "fast",
        )?])
        .await?;
        lab.reopen().await?;
        let base = lab.workspace("platform-backend").ok_or("workspace")?;
        let mut payload = base.clone();
        payload.env.clear();
        payload.role_env.insert(
            "sentinel".to_owned(),
            vec![jackin_console::domain::workspace::EnvVar::plain(
                "MODE", "fast",
            )],
        );
        lab.perform(Effect::SaveWorkspace {
            id: Some(base.id),
            base: Some(Box::new(base)),
            workspace: Box::new(payload),
        })
        .await?;
        let config = lab.config().await?;
        let saved = config.workspace(&origin).ok_or("workspace")?;
        assert!(saved.env.is_empty());
        let scoped = saved
            .role_env
            .get(&RoleName::parse("sentinel")?)
            .ok_or("role env")?;
        assert_eq!(
            scoped
                .keys()
                .map(jackin_core::ScopedEnvName::as_str)
                .collect::<Vec<_>>(),
            ["MODE"]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_secret_reference_in_env_is_refused_and_writes_nothing() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.console.config_text().await?;
        let base = lab.workspace("platform-backend").ok_or("workspace")?;
        let mut payload = base.clone();
        payload
            .env
            .push(jackin_console::domain::workspace::EnvVar::plain(
                "API_NAME",
                "op://Dev/Item/field",
            ));
        lab.perform(Effect::SaveWorkspace {
            id: Some(base.id),
            base: Some(Box::new(base)),
            workspace: Box::new(payload),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unreadable_folder_shows_the_error_inline() -> TestResult {
    guard(async {
        use std::os::unix::fs::PermissionsExt;
        let mut lab = Lab::open().await?;
        let locked = lab.code.join("locked");
        tokio::fs::create_dir_all(&locked).await?;
        tokio::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).await?;
        lab.console.keys("n down enter")?;
        let listed = lab.console.screen();
        assert!(
            listed.contains("alpha/") && listed.contains("locked/"),
            "{listed}"
        );
        lab.console.keys("down down down enter")?;
        let screen = lab.console.screen();
        tokio::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).await?;
        assert!(screen.contains("! i/o error at"), "{screen}");
        assert!(screen.contains("~/code/locked"), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn docker_toggle_writes_the_setting() -> TestResult {
    guard(async {
        let rig = jackin_testkit::Rig::builder().engine(2376).build().await?;
        let mut lab = Lab::with(rig).await?;
        let origin = name("platform-backend")?;
        lab.console.keys("down e")?;
        lab.console.click_text("Docker for the agent")?;
        let screen = lab.console.screen();
        for risk in ["Client key", "Network", "Bind mounts", "Lifetime"] {
            assert!(screen.contains(risk), "{risk}:\n{screen}");
        }
        lab.console.keys("right enter")?;
        lab.save().await?;
        let config = lab.config().await?;
        assert!(config.workspace(&origin).ok_or("workspace")?.docker);
        lab.console.keys("e")?;
        lab.console.click_text("Docker for the agent")?;
        lab.save().await?;
        let config = lab.config().await?;
        assert!(!config.workspace(&origin).ok_or("workspace")?.docker);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn editor_save_status_asks_for_a_default_role() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        lab.edit(vec![ConfigEdit::SetWorkspaceDefaultRole {
            workspace: name("platform-backend")?,
            role: None,
        }])
        .await?;
        lab.reopen().await?;
        lab.console.keys("down e 3 down space")?;
        lab.save().await?;
        let hint = fill_in(
            &Fixed::openshell().save_hint,
            &[("name", "platform-backend")],
        );
        let screen = lab.console.screen();
        assert!(screen.contains(&hint), "{hint}\n{screen}");
        let config = lab.config().await?;
        let saved = config
            .workspace(&name("platform-backend")?)
            .ok_or("workspace")?;
        assert_eq!(saved.default_role, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn editor_save_status_is_plain_with_a_default_role() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        lab.edit(vec![ConfigEdit::SetWorkspaceDefaultRole {
            workspace: name("platform-backend")?,
            role: Some(RoleName::parse("sentinel")?),
        }])
        .await?;
        lab.reopen().await?;
        lab.console.keys("down e 3 down space")?;
        lab.save().await?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("Workspace platform-backend saved"),
            "{screen}"
        );
        Ok(())
    })
    .await
}
