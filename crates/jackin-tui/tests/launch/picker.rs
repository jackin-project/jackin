// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_console::app::Route;
use jackin_console::domain::agent::Agent as ConsoleAgent;
use jackin_console::domain::launch::{LaunchContent, RefusalReason};
use jackin_console::domain::role::{NO_DEFAULT_ROLE, NO_SAVED_WORKSPACE};
use jackin_console::domain::tuple::InstanceChoice;
use jackin_console::launch::{LaunchEvent, LaunchTarget};
use jackin_core::{
    Account, AccountName, AllowedRoles, ConfigEdit, CredentialSource, EnvName, OpReference,
    Program, Provider, RoleName, WorkspaceName,
};
use jackin_testkit::{ConsoleRig, RepoFixture, Rig, TestResult, guard};

use super::support::{
    Stage, answer, begin, console, dialog, labels, only, stage, two_roles, until, until_route,
    until_shows,
};

type Failure = Box<dyn std::error::Error>;

fn env_account(name: &str, provider: Provider, var: &str) -> Result<ConfigEdit, Failure> {
    Ok(ConfigEdit::AddAccount(Account::new(
        AccountName::parse(name)?,
        provider,
        CredentialSource::Env {
            var: EnvName::parse(var)?,
        },
    )?))
}

fn words(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tokio::test]
async fn only_a_local_codex_login_is_offered() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world
            .config
            .edit(vec![
                env_account("codex-env", Provider::Codex, "OPENAI_KEY")?,
                ConfigEdit::AddAccount(Account::new(
                    AccountName::parse("codex-op")?,
                    Provider::Codex,
                    CredentialSource::OnePassword {
                        reference: OpReference::parse("op://vault/item/field")?,
                        account: None,
                    },
                )?),
                env_account("claude-env", Provider::Claude, "ANTHROPIC_KEY")?,
                env_account("grok-env", Provider::Grok, "XAI_KEY")?,
            ])
            .await?;
        let mut console = console(&rig).await?;
        console.keys("down enter")?;
        until_shows(&mut console, "Launch · choose Agent").await?;
        let screen = words(&console.screen());
        assert_eq!(
            screen.matches("usage only · launches Codex only").count(),
            2,
            "{screen}"
        );
        assert!(!screen.contains("set in Settings"), "{screen}");
        console.keys("enter")?;
        answer(&mut console, None).await?;
        until_route(&mut console, Route::Capsule).await?;
        assert_eq!(
            labels(&only(&rig)?)?.planned.agents.agents_label(),
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
                jackin_core::Agent::Codex,
                AccountName::parse("work")?
            ))
            .agents_label()
        );
        assert!(
            rig.world
                .runner
                .scripted()
                .seen()
                .iter()
                .all(|call| call.program != Program::Op)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credential_changed_under_the_picker_is_refused_typed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        console.keys("down enter")?;
        until_shows(&mut console, "Launch · choose Agent").await?;
        rig.world
            .config
            .edit(vec![ConfigEdit::SetAccountCredential {
                account: AccountName::parse("work")?,
                credential: CredentialSource::Env {
                    var: EnvName::parse("OPENAI_KEY")?,
                },
            }])
            .await?;
        console.keys("enter")?;
        until_shows(&mut console, "Run id").await?;
        let screen = console.screen();
        let shown = dialog(&screen);
        assert!(shown.contains("account work uses"), "{screen}");
        assert!(shown.contains("not supported yet"), "{screen}");
        assert_eq!(stage(&screen, "04 Credentials"), Some(Stage::Failed));
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn without_a_default_role_every_row_is_disabled() -> TestResult {
    guard(async {
        let (rig, _other) = two_roles().await?;
        let mut console = console(&rig).await?;
        console.keys("down enter")?;
        let screen = words(&console.screen());
        assert!(screen.contains(NO_DEFAULT_ROLE), "{screen}");
        assert!(screen.contains("unavailable"), "{screen}");
        console.keys("enter")?;
        console.settle().await?;
        assert_eq!(console.console().app().route(), Route::Manager);
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_only_allowed_role_is_launched() -> TestResult {
    guard(async {
        let (rig, _other) = two_roles().await?;
        let workspace = WorkspaceName::parse("platform-backend")?;
        rig.world
            .config
            .edit(vec![ConfigEdit::SetWorkspaceRoles {
                workspace,
                roles: AllowedRoles::Only(
                    jackin_core::RoleSet::new([RoleName::parse("other")?])
                        .ok_or("an empty role set")?,
                ),
            }])
            .await?;
        let mut console = console(&rig).await?;
        begin(&mut console)?;
        until_route(&mut console, Route::Capsule).await?;
        assert_eq!(labels(&only(&rig)?)?.planned.role.as_str(), "other");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_removed_under_the_picker_is_refused() -> TestResult {
    guard(async {
        let (rig, _other) = two_roles().await?;
        let workspace = WorkspaceName::parse("platform-backend")?;
        rig.world
            .config
            .edit(vec![ConfigEdit::SetWorkspaceDefaultRole {
                workspace: workspace.clone(),
                role: Some(RoleName::parse("sentinel")?),
            }])
            .await?;
        let mut console = console(&rig).await?;
        console.keys("down enter")?;
        until_shows(&mut console, "Launch · choose Agent").await?;
        rig.world
            .config
            .edit(vec![
                ConfigEdit::SetWorkspaceDefaultRole {
                    workspace: workspace.clone(),
                    role: None,
                },
                ConfigEdit::SetWorkspaceRoles {
                    workspace,
                    roles: AllowedRoles::Only(
                        jackin_core::RoleSet::new([RoleName::parse("other")?])
                            .ok_or("an empty role set")?,
                    ),
                },
            ])
            .await?;
        console.keys("enter")?;
        until_shows(&mut console, "does not allow role sentinel").await?;
        let screen = console.screen();
        let text = LaunchContent::openshell()
            .words
            .refusal(&RefusalReason::RoleNotAllowed {
                workspace: "platform-backend".to_owned(),
                role: "sentinel".to_owned(),
            });
        assert!(screen.contains(&text.title), "{}\n{screen}", text.title);
        let shown = dialog(&screen);
        for part in [&text.found, &text.state, &text.next_step] {
            assert!(shown.contains(part.as_str()), "{part}\n{screen}");
        }
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        console.keys("esc")?;
        until_route(&mut console, Route::Manager).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn another_agent_is_refused_before_any_stage() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        let app = console.console().app();
        let world = &app.world;
        let workspace = world
            .workspaces
            .iter()
            .find(|found| found.name == "platform-backend")
            .map(|found| found.id)
            .ok_or("no platform-backend workspace")?;
        let mut feed = world.backend.launch(
            world,
            &LaunchTarget {
                workspace: Some(workspace),
                role: "sentinel".to_owned(),
                instances: vec![InstanceChoice {
                    agent: ConsoleAgent::ClaudeCode,
                    provider: ConsoleAgent::ClaudeCode.provider(),
                    account: None,
                    model: String::new(),
                }],
                task: None,
            },
        );
        let mut events = Vec::new();
        for _ in 0..400 {
            events.extend(feed.poll(world));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            matches!(
                events.as_slice(),
                [LaunchEvent::Refused(RefusalReason::AgentNotLaunchable {
                    agent: ConsoleAgent::ClaudeCode,
                    launchable: ConsoleAgent::Codex,
                })]
            ),
            "{events:?}"
        );
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

async fn elsewhere(rig: &Rig) -> Result<(ConsoleRig, String, RepoFixture), Failure> {
    elsewhere_in(rig, RepoFixture::new().await?).await
}

async fn elsewhere_in(
    rig: &Rig,
    repo: RepoFixture,
) -> Result<(ConsoleRig, String, RepoFixture), Failure> {
    repo.file("README.md", b"readme\n").await?;
    repo.commit().await?;
    let base = repo
        .root()
        .as_path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or("no directory name")?;
    rig.world.seed_all(&rig.config).await?;
    rig.world
        .config
        .edit(vec![ConfigEdit::SetDefaultAccount {
            account: AccountName::parse("work")?,
        }])
        .await?;
    let console = ConsoleRig::builder(&rig.world)
        .size(160, 44)
        .cwd(repo.root().clone())
        .build()
        .await?;
    Ok((console, base, repo))
}

#[tokio::test]
async fn the_current_directory_launches_with_the_only_role() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let (mut console, base, _repo) = elsewhere(&rig).await?;
        console.skip_intro();
        console.keys("enter enter")?;
        answer(&mut console, None).await?;
        until_route(&mut console, Route::Capsule).await?;
        let found = labels(&only(&rig)?)?.planned;
        assert_eq!(found.workspace.as_str(), base);
        assert_eq!(found.role.as_str(), "sentinel");
        assert_eq!(
            found.agents.agents_label(),
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
                jackin_core::Agent::Codex,
                AccountName::parse("work")?
            ))
            .agents_label()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn launching_an_unsaved_directory_saves_its_workspace_first() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let (mut console, base, repo) = elsewhere(&rig).await?;
        console.skip_intro();
        let name = WorkspaceName::parse(&base)?;
        assert!(rig.world.config.load().await?.workspace(&name).is_none());
        console.keys("enter enter")?;
        answer(&mut console, None).await?;
        until_route(&mut console, Route::Capsule).await?;
        let saved = rig.world.config.load().await?;
        let workspace = saved
            .workspace(&name)
            .ok_or("the workspace was not saved")?;
        assert_eq!(&workspace.path, repo.root());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_current_directory_reached_through_a_symlink_finds_the_saved_workspace() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let repo = RepoFixture::new().await?;
        repo.file("README.md", b"readme\n").await?;
        repo.commit().await?;
        let alias = rig.world.home().as_path().join("alias");
        std::os::unix::fs::symlink(repo.root().as_path(), &alias)?;
        rig.world.seed_all(&rig.config).await?;
        rig.world
            .config
            .edit(vec![
                ConfigEdit::SetDefaultAccount {
                    account: AccountName::parse("work")?,
                },
                ConfigEdit::AddWorkspace {
                    name: WorkspaceName::parse("saved")?,
                    workspace: jackin_core::Workspace::new(repo.root().clone(), false),
                },
            ])
            .await?;
        let mut console = ConsoleRig::builder(&rig.world)
            .size(160, 44)
            .cwd(jackin_core::HostPath::parse(&alias)?)
            .build()
            .await?;
        console.skip_intro();
        console.keys("enter enter")?;
        until(
            &mut console,
            "the values form or a failed image stage",
            |c| {
                let screen = c.screen();
                screen.contains("Values for this launch")
                    || stage(&screen, "01 Image build") == Some(Stage::Failed)
            },
        )
        .await?;
        let screen = console.screen();
        assert!(screen.contains("Values for this launch"), "{screen}");
        answer(&mut console, None).await?;
        until_route(&mut console, Route::Capsule).await?;
        assert_eq!(labels(&only(&rig)?)?.planned.workspace.as_str(), "saved");
        let saved = rig.world.config.load().await?;
        assert_eq!(saved.workspaces().count(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_directory_whose_name_is_not_a_workspace_name_launches_under_a_derived_name() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        let (mut console, _, _repo) =
            elsewhere_in(&rig, RepoFixture::new_named("My App").await?).await?;
        console.skip_intro();
        console.keys("enter enter")?;
        answer(&mut console, None).await?;
        until_route(&mut console, Route::Capsule).await?;
        let found = labels(&only(&rig)?)?.planned;
        assert_eq!(found.workspace.as_str(), "my-app");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_current_directory_with_several_roles_asks_for_a_workspace() -> TestResult {
    guard(async {
        let (rig, _other) = two_roles().await?;
        let (mut console, _, _repo) = elsewhere(&rig).await?;
        console.skip_intro();
        console.keys("enter")?;
        let screen = words(&console.screen());
        let shown: String = NO_SAVED_WORKSPACE.chars().take(40).collect();
        assert!(screen.contains(&shown), "{screen}");
        assert!(screen.contains("unavailable"), "{screen}");
        console.keys("enter")?;
        console.settle().await?;
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}
