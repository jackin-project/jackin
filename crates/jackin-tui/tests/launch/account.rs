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

use super::support::{console, labels, only};

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
            agent: Agent::Omp,
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
