// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_console::domain::agent::{Agent as ConsoleAgent, Provider as ConsoleProvider};
use jackin_console::domain::launch::RefusalReason;
use jackin_console::domain::tuple::InstanceChoice;
use jackin_console::launch::{LaunchEvent, LaunchTarget};
use jackin_core::{Account, AccountName, Agent, ConfigEdit, CredentialSource, HostPath, Provider};
use jackin_testkit::{ConsoleRig, Rig, RoleFixture, TestResult, guard};

use super::support::{console, only};

const ROLE: &str = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n\n[docker]\nenabled = false\n";

fn role() -> Result<RoleFixture, Box<dyn std::error::Error>> {
    Ok(RoleFixture::new("sentinel")?.manifest_text(ROLE.to_owned()))
}

async fn launch(
    console: &mut ConsoleRig,
    agent: ConsoleAgent,
    provider: ConsoleProvider,
) -> Result<Vec<LaunchEvent>, Box<dyn std::error::Error>> {
    let app = console.console().app();
    let world = &app.world;
    let workspace = world
        .workspaces
        .iter()
        .find(|found| found.name == "platform-backend")
        .map(|found| found.id)
        .ok_or("platform-backend workspace")?;
    let mut feed = world.backend.launch(
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
    );
    let mut events = Vec::new();
    for _ in 0..4000 {
        events.extend(feed.poll(world));
        if events.iter().any(|event| {
            matches!(
                event,
                LaunchEvent::Ready(_) | LaunchEvent::Failed(_) | LaunchEvent::Refused(_)
            )
        }) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    Ok(events)
}

async fn add_zai_account(rig: &Rig) -> Result<(), Box<dyn std::error::Error>> {
    let path = HostPath::parse(&rig.home.as_path().join("zai.key"))?;
    tokio::fs::write(path.as_path(), "test-key").await?;
    rig.world
        .config
        .edit(vec![
            ConfigEdit::AddAccount(Account::new(
                AccountName::parse("zai-work")?,
                Provider::Zai,
                CredentialSource::File { path },
            )?),
            ConfigEdit::SetDefaultAccount {
                account: AccountName::parse("zai-work")?,
            },
        ])
        .await?;
    Ok(())
}

#[tokio::test]
async fn supported_non_codex_tuples_launch_with_blank_model_defaults() -> TestResult {
    guard(async {
        for agent in [
            ConsoleAgent::Omp,
            ConsoleAgent::ClaudeCode,
            ConsoleAgent::Codex,
        ] {
            let rig = Rig::builder().role(role()?).build().await?;
            let mut console = console(&rig).await?;
            add_zai_account(&rig).await?;
            let events = launch(&mut console, agent, ConsoleProvider::Zai).await?;
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, LaunchEvent::Ready(_))),
                "{agent:?}: {events:?}"
            );
            let handle = only(&rig)?;
            let instances: jackin_core::AgentInstances = serde_json::from_slice(
                &handle
                    .file(&jackin_core::SandboxPath::from_static(
                        "/sandbox/.jackin/agents.json",
                    ))
                    .ok_or("no agents.json")?,
            )?;
            let lead = instances.lead();
            let expected_agent = match agent {
                ConsoleAgent::Omp => Agent::Omp,
                ConsoleAgent::ClaudeCode => Agent::ClaudeCode,
                ConsoleAgent::Codex => Agent::Codex,
                _ => unreachable!(),
            };
            assert_eq!(lead.agent, expected_agent);
            assert_eq!(lead.provider, Provider::Zai);
            assert_eq!(lead.account.as_str(), "zai-work");
            assert_eq!(lead.model, "glm-5.3");
            assert_eq!(rig.gateway().sandboxes().len(), 1);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unavailable_and_blocked_tuples_retain_exact_reasons_without_effects() -> TestResult {
    guard(async {
        for (agent, provider, expected_agent, expected_provider, reason) in [(
            ConsoleAgent::ClaudeCode,
            ConsoleProvider::Anthropic,
            Agent::ClaudeCode,
            Provider::Claude,
            "vendor clarification pending",
        )] {
            let rig = Rig::new().await?;
            let mut console = console(&rig).await?;
            let events = launch(&mut console, agent, provider).await?;
            let expected = format!(
                "tuple {} with provider {} is not supported: {reason}",
                expected_agent.as_str(),
                expected_provider.as_str()
            );
            let failure = events.iter().find_map(|event| match event {
                LaunchEvent::Failed(failed) => Some(failed),
                _ => None,
            });
            assert_eq!(
                failure.map(|failed| failed.failure.summary.as_str()),
                Some(expected.as_str()),
                "{events:?}"
            );
            assert!(!events.iter().any(|event| matches!(
                event,
                LaunchEvent::Refused(RefusalReason::AgentNotLaunchable { .. })
            )));
            assert!(rig.gateway().sandboxes().is_empty());
            assert!(rig.gateway().providers().is_empty());
            assert!(rig.world.engine.builds().is_empty());
        }
        Ok(())
    })
    .await
}
