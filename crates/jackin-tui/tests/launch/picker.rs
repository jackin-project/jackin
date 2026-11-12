// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_console::app::Route;
use jackin_console::domain::agent::{Agent as ConsoleAgent, Provider as ConsoleProvider};
use jackin_console::domain::launch::{LaunchContent, RefusalReason};
use jackin_console::domain::role::{NO_DEFAULT_ROLE, NO_SAVED_WORKSPACE};
use jackin_console::domain::tuple::{InstanceChoice, TupleState as ConsoleTupleState};
use jackin_console::launch::{EnvAnswer, LaunchEvent, LaunchTarget};
use jackin_core::{
    Account, AccountName, AllowedRoles, ConfigEdit, CredentialSource, EnvName, OpReference,
    Program, Provider, RoleName, RoleSource, RoleUrl, WorkspaceName,
};
use jackin_testkit::{ConsoleRig, FsFault, RepoFixture, Rig, RoleFixture, TestResult, guard};

use super::support::{
    Stage, answer, console, dialog, direct_launch, labels, only, picker_account, stage, two_roles,
    until, until_route, until_shows,
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
        picker_account(&rig).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world
            .config
            .edit(vec![
                env_account("codex-env", Provider::Codex, "OPENAI_KEY")?,
                ConfigEdit::AddAccount(Account::new(
                    AccountName::parse("codex-local")?,
                    Provider::Codex,
                    CredentialSource::CodexLogin {
                        home: rig.home.clone(),
                        chatgpt_account_id: jackin_core::ChatgptAccountId::parse(
                            jackin_testkit::ACCOUNT_ID,
                        )?,
                    },
                )?),
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
        assert!(!screen.contains("final live proof pending"), "{screen}");
        assert!(!screen.contains("host Codex login expired"), "{screen}");
        assert!(screen.contains("Claude Code"), "{screen}");
        assert!(screen.contains("Grok Build"), "{screen}");
        assert!(!screen.contains("set in Settings"), "{screen}");
        let world = &console.console().app().world;
        let workspace = world
            .workspaces
            .iter()
            .find(|found| found.name == "platform-backend")
            .map(|found| found.id)
            .ok_or("no platform-backend workspace")?;
        let events = direct_launch(
            world,
            &LaunchTarget {
                workspace: Some(workspace),
                role: "sentinel".to_owned(),
                instances: vec![InstanceChoice {
                    agent: ConsoleAgent::Codex,
                    provider: ConsoleProvider::OpenAi,
                    account: Some("codex-local".to_owned()),
                    model: "gpt-6.1-sol".to_owned(),
                }],
                task: None,
            },
        )
        .await?;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LaunchEvent::Ready(_))),
            "{events:?}"
        );
        assert_eq!(
            labels(&only(&rig)?)?.planned.agents,
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                jackin_core::Agent::Codex,
                Provider::Codex,
                AccountName::parse("codex-local")?,
                "gpt-6.1-sol".to_owned(),
            ))
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
async fn a_git_sourced_role_launches_from_the_picker() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let fixture = RoleFixture::new("sentinel")?;
        fixture.commit().await?;
        let url = "https://example.invalid/role.git";
        tokio::fs::write(
            rig.world.home().as_path().join(".gitconfig"),
            format!(
                "[url \"file://{}\"]\n\tinsteadOf = {url}\n",
                fixture.root().as_path().display()
            ),
        )
        .await?;
        rig.world
            .config
            .edit(vec![
                ConfigEdit::RemoveRole(RoleName::parse("sentinel")?),
                ConfigEdit::AddRole {
                    name: RoleName::parse("sentinel")?,
                    source: RoleSource::Git {
                        url: RoleUrl::parse(url)?,
                        rev: None,
                    },
                },
                ConfigEdit::SetWorkspaceDefaultRole {
                    workspace: WorkspaceName::parse("platform-backend")?,
                    role: Some(RoleName::parse("sentinel")?),
                },
            ])
            .await?;
        let mut console = console(&rig).await?;
        console.keys("down enter")?;
        until_shows(&mut console, "Launch · choose Agent").await?;
        console.keys("space enter")?;
        until_route(&mut console, Route::Capsule).await?;
        let found = labels(&only(&rig)?)?.planned;
        assert_eq!(found.role.as_str(), "sentinel");
        assert_eq!(
            found.agents.agents_label(),
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                jackin_core::Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ))
            .agents_label()
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
        rig.world
            .config
            .edit(vec![ConfigEdit::SetAccountCredential {
                account: AccountName::parse("work")?,
                credential: CredentialSource::Env {
                    var: EnvName::parse("OPENAI_KEY")?,
                },
            }])
            .await?;
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
                    agent: ConsoleAgent::Codex,
                    provider: ConsoleProvider::Zai,
                    account: Some("work".to_owned()),
                    model: "glm-5.3".to_owned(),
                }],
                task: None,
            },
        );
        let mut events = Vec::new();
        for _ in 0..1000 {
            events.extend(feed.poll(world));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        feed.answer_env(vec![EnvAnswer {
            name: "GREETING".to_owned(),
            value: "hello".to_owned(),
        }]);
        for _ in 0..1000 {
            events.extend(feed.poll(world));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            events.iter().any(|event| matches!(
                event,
                LaunchEvent::Failed(failed) if failed.failure.summary.contains("no credential is available for account work")
            )),
            "{events:?}"
        );
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
        let shown = NO_DEFAULT_ROLE.split(" · ").next().ok_or("role reason")?;
        assert!(screen.contains(shown), "{screen}");
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
        super::support::begin_role(&mut console, "other")?;
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
        console.keys("space enter")?;
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
async fn blocked_vendor_agent_is_refused_before_any_stage() -> TestResult {
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
            events.iter().any(|event| matches!(
                event,
                LaunchEvent::Failed(failed)
                    if failed.failure.summary == "tuple claude with provider claude is not supported: vendor clarification pending"
            )),
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
        console.keys("enter space enter")?;
        answer(&mut console, None).await?;
        until_route(&mut console, Route::Capsule).await?;
        let found = labels(&only(&rig)?)?.planned;
        assert_eq!(found.workspace.as_str(), base);
        assert_eq!(found.role.as_str(), "sentinel");
        assert_eq!(
            found.agents.agents_label(),
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                jackin_core::Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
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
        console.keys("enter space enter")?;
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

fn adhoc_target() -> LaunchTarget {
    LaunchTarget {
        workspace: None,
        role: "sentinel".to_owned(),
        instances: vec![InstanceChoice {
            agent: ConsoleAgent::Codex,
            provider: ConsoleProvider::Zai,
            account: Some("work".to_owned()),
            model: "glm-5.3".to_owned(),
        }],
        task: None,
    }
}

fn ready_warning(
    events: &[LaunchEvent],
) -> Result<Option<jackin_console::world::SaveWarning>, Failure> {
    events
        .iter()
        .find_map(|event| match event {
            LaunchEvent::Ready(ready) => Some(ready.warning),
            _ => None,
        })
        .ok_or_else(|| format!("launch was not ready: {events:?}").into())
}

#[tokio::test]
async fn ordinary_ad_hoc_launch_has_no_save_warning() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let (mut console, base, repo) = elsewhere(&rig).await?;
        let world = &console.console().app().world;
        let events = direct_launch(world, &adhoc_target()).await?;
        assert_eq!(ready_warning(&events)?, None);
        let name = WorkspaceName::parse(&base)?;
        let saved = rig.world.config.load().await?;
        assert_eq!(
            saved.workspace(&name).map(|workspace| &workspace.path),
            Some(repo.root())
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_unsynced_ad_hoc_launch_is_ready_with_a_warning() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let (mut console, base, repo) = elsewhere(&rig).await?;
        rig.world.fail_fs(FsFault::SyncDir);
        let world = &console.console().app().world;
        let events = direct_launch(world, &adhoc_target()).await?;
        assert_eq!(
            ready_warning(&events)?,
            Some(jackin_console::world::SaveWarning::DirectorySync)
        );
        let name = WorkspaceName::parse(&base)?;
        let saved = rig.world.config.load().await?;
        assert_eq!(
            saved.workspace(&name).map(|workspace| &workspace.path),
            Some(repo.root())
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn pre_commit_ad_hoc_failure_creates_no_workspace_or_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let (mut console, base, _repo) = elsewhere(&rig).await?;
        rig.world.fail_fs(FsFault::WriteNew);
        let world = &console.console().app().world;
        let events = direct_launch(world, &adhoc_target()).await?;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LaunchEvent::Failed(_))),
            "{events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, LaunchEvent::Ready(_)))
        );
        let name = WorkspaceName::parse(&base)?;
        assert!(rig.world.config.load().await?.workspace(&name).is_none());
        assert!(rig.gateway().sandboxes().is_empty());
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
        console.keys("enter space enter")?;
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
        console.keys("enter space enter")?;
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
        let shown = NO_SAVED_WORKSPACE
            .split(" · ")
            .next()
            .ok_or("workspace reason")?;
        assert!(screen.contains(shown), "{screen}");
        assert!(screen.contains("unavailable"), "{screen}");
        console.keys("enter")?;
        console.settle().await?;
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

async fn availability_rig() -> Result<Rig, Failure> {
    let role = RoleFixture::new("sentinel")?.manifest_text(
        "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n".to_owned(),
    );
    let rig = Rig::builder().role(role).build().await?;
    picker_account(&rig).await?;
    Ok(rig)
}

fn console_agent(agent: jackin_core::Agent) -> Option<ConsoleAgent> {
    match agent {
        jackin_core::Agent::Codex => Some(ConsoleAgent::Codex),
        jackin_core::Agent::ClaudeCode => Some(ConsoleAgent::ClaudeCode),
        jackin_core::Agent::Omp => Some(ConsoleAgent::Omp),
        _ => None,
    }
}

fn console_provider(provider: Provider) -> ConsoleProvider {
    match provider {
        Provider::Claude => ConsoleProvider::Anthropic,
        Provider::Codex => ConsoleProvider::OpenAi,
        Provider::Kimi => ConsoleProvider::Moonshot,
        Provider::Minimax => ConsoleProvider::MiniMax,
        Provider::Mimo => ConsoleProvider::Mimo,
        Provider::Zai => ConsoleProvider::Zai,
        _ => unreachable!("only offered providers are converted"),
    }
}

#[tokio::test]
async fn tuple_offers_match_the_complete_supported_matrix_and_remaining_controls() -> TestResult {
    guard(async {
        let rig = availability_rig().await?;
        let mut console = console(&rig).await?;
        let offers = &console.console().app().world.surfaces.tuples;
        for tuple in jackin_core::TUPLES {
            let Some(agent) = console_agent(tuple.agent) else {
                continue;
            };
            let provider = console_provider(tuple.provider);
            let offer = offers
                .iter()
                .find(|offer| offer.agent == agent && offer.provider == provider)
                .ok_or_else(|| format!("missing {} offer", tuple.id))?;
            let expected_state = match tuple.id {
                "T02" => ConsoleTupleState::BlockedVendor,
                "T22" => ConsoleTupleState::Unavailable,
                _ => ConsoleTupleState::Supported,
            };
            let expected_reason = match tuple.id {
                "T02" => Some("vendor clarification pending"),
                "T22" => Some("no API key on this host"),
                _ => None,
            };
            assert_eq!(offer.state, expected_state, "{}", tuple.id);
            assert_eq!(offer.reason.as_deref(), expected_reason, "{}", tuple.id);
            assert_eq!(
                offer.allowed_models,
                [tuple.default_model.to_owned()],
                "{}",
                tuple.id
            );
            assert_eq!(offer.default_model, tuple.default_model, "{}", tuple.id);
        }
        Ok(())
    })
    .await
}

async fn promoted_console_launch(
    agent: jackin_core::Agent,
    provider: Provider,
    account: &str,
    model: Option<&str>,
) -> TestResult {
    let tuple = jackin_core::lookup_tuple(agent, provider)?;
    let expected_model = model.unwrap_or(tuple.default_model);
    let rig = availability_rig().await?;
    if matches!(provider, Provider::Kimi | Provider::Mimo) {
        rig.world
            .config
            .edit(vec![ConfigEdit::AddAccount(Account::new(
                AccountName::parse(account)?,
                provider,
                CredentialSource::File {
                    path: jackin_core::HostPath::parse(&rig.home.as_path().join("zai.txt"))?,
                },
            )?)])
            .await?;
    }
    let mut console = console(&rig).await?;
    let world = &console.console().app().world;
    let workspace = world
        .workspaces
        .iter()
        .find(|found| found.name == "platform-backend")
        .map(|found| found.id)
        .ok_or("no platform-backend workspace")?;
    let events = direct_launch(
        world,
        &LaunchTarget {
            workspace: Some(workspace),
            role: "sentinel".to_owned(),
            instances: vec![InstanceChoice {
                agent: console_agent(agent).ok_or("unsupported console agent")?,
                provider: console_provider(provider),
                account: Some(account.to_owned()),
                model: model.unwrap_or_default().to_owned(),
            }],
            task: None,
        },
    )
    .await?;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LaunchEvent::Ready(_))),
        "{events:?}"
    );
    let handle = only(&rig)?;
    assert_eq!(
        labels(&handle)?.planned.agents,
        jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
            agent,
            provider,
            AccountName::parse(account)?,
            expected_model.to_owned(),
        ))
    );
    Ok(())
}

#[tokio::test]
async fn t01_console_launch_is_ready_with_exact_chatgpt_labels() -> TestResult {
    guard(promoted_console_launch(
        jackin_core::Agent::Codex,
        Provider::Codex,
        "picker-codex",
        Some("gpt-6.1-sol"),
    ))
    .await
}

#[tokio::test]
async fn t20_console_launch_is_ready_with_exact_mimo_labels() -> TestResult {
    guard(promoted_console_launch(
        jackin_core::Agent::Codex,
        Provider::Mimo,
        "mimo",
        Some("mimo-v2.6-pro"),
    ))
    .await
}

#[tokio::test]
async fn t20_blank_model_console_launch_uses_the_tuple_default() -> TestResult {
    guard(promoted_console_launch(
        jackin_core::Agent::Codex,
        Provider::Mimo,
        "mimo",
        None,
    ))
    .await
}

async fn unavailable_launch(
    agent: ConsoleAgent,
    provider: ConsoleProvider,
    agent_name: &str,
    provider_name: &str,
    reason: &str,
) -> TestResult {
    guard(async {
        let rig = availability_rig().await?;
        let mut console = console(&rig).await?;
        let world = &console.console().app().world;
        let workspace = world
            .workspaces
            .iter()
            .find(|found| found.name == "platform-backend")
            .map(|found| found.id)
            .ok_or("no platform-backend workspace")?;
        let events = direct_launch(
            world,
            &LaunchTarget {
                workspace: Some(workspace),
                role: "sentinel".to_owned(),
                instances: vec![InstanceChoice {
                    agent,
                    provider,
                    account: None,
                    model: String::new(),
                }],
                task: None,
            },
        )
        .await?;
        let expected =
            format!("tuple {agent_name} with provider {provider_name} is not supported: {reason}");
        assert!(
            events.iter().any(|event| matches!(
                event,
                LaunchEvent::Failed(failed) if failed.failure.summary == expected
            )),
            "{events:?}"
        );
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn t13_console_launch_is_ready_with_exact_kimi_labels() -> TestResult {
    guard(promoted_console_launch(
        jackin_core::Agent::ClaudeCode,
        Provider::Kimi,
        "kimi",
        Some("kimi-for-coding"),
    ))
    .await
}

#[tokio::test]
async fn t22_launch_refusal_has_the_exact_missing_key_reason_and_no_effects() -> TestResult {
    unavailable_launch(
        ConsoleAgent::Omp,
        ConsoleProvider::OpenAi,
        "omp",
        "codex",
        "no API key on this host",
    )
    .await
}

#[tokio::test]
async fn t18_console_launch_is_the_positive_control() -> TestResult {
    guard(async {
        let rig = availability_rig().await?;
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
                    agent: ConsoleAgent::Codex,
                    provider: ConsoleProvider::Zai,
                    account: Some("work".to_owned()),
                    model: "glm-5.3".to_owned(),
                }],
                task: None,
            },
        );
        let mut events = Vec::new();
        for _ in 0..1000 {
            events.extend(feed.poll(world));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LaunchEvent::Ready(_))),
            "{events:?}"
        );
        let handle = only(&rig)?;
        assert_eq!(
            labels(&handle)?.planned.agents.agents_label(),
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                jackin_core::Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ))
            .agents_label()
        );
        Ok(())
    })
    .await
}
