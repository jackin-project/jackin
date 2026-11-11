// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_console::domain::agent::{Agent as ConsoleAgent, Provider as ConsoleProvider};
use jackin_console::domain::tuple::InstanceChoice;
use jackin_console::launch::{EnvAnswer, LaunchEvent, LaunchTarget};
use jackin_core::{
    Account, AccountName, AccountQuery, Agent, ConfigEdit, CredentialSource, Provider, RoleName,
    WorkspaceName,
};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{console, labels, only, picker_account};

type Failure = Box<dyn std::error::Error>;

const WORKSPACE: &str = "platform-backend";

fn name(text: &str) -> Result<AccountName, Failure> {
    Ok(AccountName::parse(text)?)
}

fn login(rig: &Rig, account: &str) -> Result<ConfigEdit, Failure> {
    Ok(ConfigEdit::AddAccount(Account::new(
        name(account)?,
        Provider::Zai,
        CredentialSource::File {
            path: jackin_core::HostPath::parse(&rig.home.as_path().join("zai.txt"))?,
        },
    )?))
}

async fn launched(rig: &Rig, expected: &str) -> Result<String, Failure> {
    let mut console = console(rig).await?;
    let app = console.console().app();
    let world = &app.world;
    let workspace = world
        .workspaces
        .iter()
        .find(|found| found.name == WORKSPACE)
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
                account: Some(expected.to_owned()),
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
        events
            .iter()
            .any(|event| matches!(event, LaunchEvent::Ready(_))),
        "{events:?}"
    );
    Ok(labels(&only(rig)?)?.planned.agents.agents_label())
}

async fn failed_launch(rig: &Rig, account: &str) -> Result<Vec<LaunchEvent>, Failure> {
    let mut console = console(rig).await?;
    let app = console.console().app();
    let world = &app.world;
    let workspace = world
        .workspaces
        .iter()
        .find(|found| found.name == WORKSPACE)
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
                account: Some(account.to_owned()),
                model: "glm-5.3".to_owned(),
            }],
            task: None,
        },
    );
    let mut events = Vec::new();
    for _ in 0..400 {
        events.extend(feed.poll(world));
        if events
            .iter()
            .any(|event| matches!(event, LaunchEvent::Failed(_)))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok(events)
}

fn assert_failed_at_credentials(events: &[LaunchEvent], expected: &str) {
    assert!(
        events.iter().any(|event| matches!(
            event,
            LaunchEvent::Failed(failed)
                if failed.failure.stage == 3 && failed.failure.summary == expected
        )),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, LaunchEvent::Ready(_) | LaunchEvent::NeedsInput(_))),
        "{events:?}"
    );
}

fn assert_no_effects(rig: &Rig) {
    assert!(rig.world.engine.builds().is_empty());
    assert!(rig.world.gateway.providers().is_empty());
    assert!(rig.world.gateway.sandboxes().is_empty());
}

async fn launches(
    expected: &str,
    workspace: bool,
    role: bool,
    default: bool,
) -> Result<(), Failure> {
    let rig = Rig::new().await?;
    rig.world.seed_all(&rig.config).await?;
    let mut edits = Vec::new();
    for account in ["by-workspace", "by-role", "by-default"] {
        edits.push(login(&rig, account)?);
    }
    if workspace {
        edits.push(ConfigEdit::BindWorkspaceAccount {
            workspace: WorkspaceName::parse(WORKSPACE)?,
            account: name("by-workspace")?,
        });
    }
    if default {
        edits.push(ConfigEdit::SetDefaultAccount {
            account: name("by-default")?,
        });
    }
    if role {
        edits.push(ConfigEdit::BindRoleAccount {
            role: RoleName::parse("sentinel")?,
            agent: Agent::Codex,
            account: name("by-role")?,
        });
    }
    rig.world.config.edit(edits).await?;
    let config = rig.world.config.load().await?;
    let workspace_name = WorkspaceName::parse(WORKSPACE)?;
    let role_name = RoleName::parse("sentinel")?;
    let selected = config.select_account(&AccountQuery {
        flag: None,
        workspace: &workspace_name,
        role: &role_name,
        agent: Agent::Codex,
        provider: Provider::Zai,
    })?;
    assert_eq!(selected.account.as_str(), expected);
    let account = launched(&rig, expected).await?;
    assert_eq!(
        account,
        jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
            Agent::Codex,
            Provider::Zai,
            name(expected)?,
            "glm-5.3".to_owned(),
        ))
        .agents_label()
    );
    Ok(())
}

#[tokio::test]
async fn the_workspace_account_is_launched_first() -> TestResult {
    guard(async {
        launches("by-workspace", true, true, true).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_role_account_is_launched_without_a_workspace_account() -> TestResult {
    guard(async {
        launches("by-role", false, true, true).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_default_account_is_launched_without_a_binding() -> TestResult {
    guard(async {
        launches("by-default", false, false, true).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_explicit_non_default_zai_account_is_selected_exactly() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world
            .config
            .edit(vec![
                login(&rig, "by-explicit")?,
                login(&rig, "by-default")?,
                ConfigEdit::SetDefaultAccount {
                    account: name("by-default")?,
                },
            ])
            .await?;
        let selected = launched(&rig, "by-explicit").await?;
        assert_eq!(
            selected,
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                name("by-explicit")?,
                "glm-5.3".to_owned(),
            ))
            .agents_label()
        );
        let providers = rig
            .world
            .gateway
            .providers()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(providers, ["jackin-by-explicit-codex"]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_explicit_unknown_account_fails_at_credentials_without_effects() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let events = failed_launch(&rig, "ghost").await?;
        assert_failed_at_credentials(&events, "unknown account ghost");
        assert_no_effects(&rig);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_explicit_wrong_provider_account_fails_at_credentials_without_effects() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        picker_account(&rig).await?;
        let events = failed_launch(&rig, "picker-codex").await?;
        assert_failed_at_credentials(
            &events,
            "account picker-codex does not belong to the zai provider",
        );
        assert_no_effects(&rig);
        Ok(())
    })
    .await
}
