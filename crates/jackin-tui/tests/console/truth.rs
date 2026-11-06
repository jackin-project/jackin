// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::account::CredentialSource as Source;
use jackin_console::domain::workspace::AllowedRoles;
use jackin_core::{Account, AccountName, ConfigEdit, CredentialSource, EnvName, Provider};
use jackin_testkit::{ConsoleRig, Rig, TestResult, guard};

async fn console() -> Result<(Rig, ConsoleRig), Box<dyn std::error::Error>> {
    let rig = Rig::new().await?;
    rig.world.seed_all(&rig.config).await?;
    let second = Account::new(
        AccountName::parse("spare")?,
        Provider::Codex,
        CredentialSource::Env {
            var: EnvName::parse("SPARE_KEY")?,
        },
    )?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddAccount(second)])
        .await?;
    let mut console = ConsoleRig::new(&rig.world).await?;
    console.skip_intro();
    Ok((rig, console))
}

#[tokio::test]
async fn one_project_row() -> TestResult {
    guard(async {
        let (_rig, mut console) = console().await?;
        let world = &console.console().app().world;
        let ws = &world.workspaces[0];
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].destination, "/sandbox/project");
        assert!(!ws.mounts[0].readonly);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn policies_row() -> TestResult {
    guard(async {
        let (_rig, mut console) = console().await?;
        console.keys("down")?;
        let screen = console.screen();
        assert!(
            screen.contains("git pull disabled · keep awake off · dirty exit ask"),
            "{screen}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn allow_list_shows_k_of_m() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world
            .config
            .edit(vec![ConfigEdit::SetWorkspaceRoles {
                workspace: jackin_core::WorkspaceName::parse("platform-backend")?,
                roles: jackin_core::AllowedRoles::Only(
                    jackin_core::RoleSet::new([jackin_core::RoleName::parse("sentinel")?])
                        .ok_or("empty role set")?,
                ),
            }])
            .await?;
        let mut console = ConsoleRig::new(&rig.world).await?;
        console.skip_intro();
        assert_eq!(
            console.console().app().world.workspaces[0].roles.allowed,
            AllowedRoles::Custom(vec!["sentinel".to_owned()])
        );
        console.keys("down")?;
        let screen = console.screen();
        assert!(screen.contains("allowed 1 of 1"), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn working_directory() -> TestResult {
    guard(async {
        let (_rig, mut console) = console().await?;
        console.keys("down")?;
        let screen = console.screen();
        assert!(
            screen.contains("Working dir   /sandbox/project"),
            "{screen}"
        );
        console.keys("e")?;
        let editor = console.screen();
        assert!(editor.contains("/sandbox/project"), "{editor}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn no_plain_key_source() -> TestResult {
    guard(async {
        let (_rig, mut console) = console().await?;
        let accounts = &console.console().app().world.accounts.accounts;
        assert_eq!(accounts.len(), 2);
        assert!(
            accounts
                .iter()
                .all(|a| !matches!(a.source, Source::PlainApiKey { .. }))
        );
        assert!(accounts.iter().all(|a| a.enabled));
        Ok(())
    })
    .await
}
