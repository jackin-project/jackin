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
        assert!(screen.contains("final live proof pending"), "{screen}");
        assert!(!screen.contains("host Codex login expired"), "{screen}");
        assert!(screen.contains("Claude Code"), "{screen}");
        assert!(screen.contains("Grok Build"), "{screen}");
        assert!(!screen.contains("set in Settings"), "{screen}");
        console.keys("space enter")?;
        answer(&mut console, None).await?;
        until_route(&mut console, Route::Capsule).await?;
        assert_eq!(
            labels(&only(&rig)?)?.planned.agents.agents_label(),
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                jackin_core::Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
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
async fn an_out_of_catalog_agent_is_refused_before_any_stage() -> TestResult {
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
                    agent: ConsoleAgent::Amp,
                    provider: ConsoleAgent::Amp.provider(),
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
                    agent: ConsoleAgent::Amp,
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

async fn unavailable_offer(
    agent: ConsoleAgent,
    provider: ConsoleProvider,
    reason: &str,
) -> TestResult {
    guard(async {
        let rig = availability_rig().await?;
        let mut console = console(&rig).await?;
        let offers = &console.console().app().world.surfaces.tuples;
        let offer = offers
            .iter()
            .find(|offer| offer.agent == agent && offer.provider == provider)
            .ok_or("missing tuple offer")?;
        assert_eq!(offer.state, ConsoleTupleState::Unavailable);
        assert_eq!(offer.reason.as_deref(), Some(reason));
        Ok(())
    })
    .await
}

macro_rules! unavailable_offer_tests {
    ($($name:ident => ($agent:ident, $provider:ident, $reason:literal)),+ $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() -> TestResult {
                unavailable_offer(ConsoleAgent::$agent, ConsoleProvider::$provider, $reason).await
            }
        )+
    };
}

unavailable_offer_tests! {
    t01_offer_is_unavailable => (Codex, OpenAi, "final live proof pending after host re-login"),
    t10_offer_is_unavailable => (Omp, Moonshot, "final live proof pending with Kimi plan key"),
    t11_offer_is_unavailable => (Omp, MiniMax, "final live proof pending with MiniMax plan key"),
    t12_offer_is_unavailable => (Omp, Mimo, "final live proof pending with MiMo Token Plan key"),
    t16_offer_is_unavailable => (ClaudeCode, Mimo, "final live proof pending with MiMo Token Plan key"),
    t20_offer_is_unavailable => (Codex, Mimo, "final live proof pending with MiMo Token Plan key"),
}

async fn unavailable_launch(provider: ConsoleProvider, reason: &str) -> TestResult {
    guard(async {
        let rig = availability_rig().await?;
        if provider == ConsoleProvider::Mimo {
            rig.world
                .config
                .edit(vec![env_account("mimo", Provider::Mimo, "MIMO_KEY")?])
                .await?;
        }
        let mut console = console(&rig).await?;
        let app = console.console().app();
        let world = &app.world;
        let workspace = world
            .workspaces
            .iter()
            .find(|found| found.name == "platform-backend")
            .map(|found| found.id)
            .ok_or("no platform-backend workspace")?;
        let account = (provider == ConsoleProvider::Mimo).then(|| "mimo".to_owned());
        let provider_name = match provider {
            ConsoleProvider::OpenAi => "codex",
            ConsoleProvider::Mimo => "mimo",
            _ => return Err("unexpected console provider".into()),
        };
        let mut feed = world.backend.launch(
            world,
            &LaunchTarget {
                workspace: Some(workspace),
                role: "sentinel".to_owned(),
                instances: vec![InstanceChoice {
                    agent: ConsoleAgent::Codex,
                    provider,
                    account,
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
        let expected =
            format!("tuple codex with provider {provider_name} is not supported: {reason}");
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
async fn t01_launch_refusal_has_the_exact_reason_and_no_effects() -> TestResult {
    unavailable_launch(
        ConsoleProvider::OpenAi,
        "final live proof pending after host re-login",
    )
    .await
}

#[tokio::test]
async fn t20_launch_refusal_has_the_exact_reason_and_no_effects() -> TestResult {
    unavailable_launch(
        ConsoleProvider::Mimo,
        "final live proof pending with MiMo Token Plan key",
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
