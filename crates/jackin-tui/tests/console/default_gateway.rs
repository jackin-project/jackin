// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_console::app::Route;
use jackin_console::backend::Effect;
use jackin_console::domain::account::{Account, CredentialSource, DetectedKind, UNSAVED_ID};
use jackin_console::domain::agent::Provider;
use jackin_core::{
    AccountName, Agent, ConfigEdit, GatewayName, HookKind, OpenShellWorkspace, ProviderName,
    sandbox_layout,
};
use jackin_openshell::Gateway;
use jackin_session::CheckId;
use jackin_testkit::{
    ConsoleRig, FakeGateway, PLACEHOLDER, ProgramScript, ScriptedConnector, TestResult,
    connector_over, guard, session_over,
};
use jackin_tui::GatewayState;
use tokio_util::sync::CancellationToken;

use super::support::Lab;

type Out<T> = Result<T, Box<dyn std::error::Error>>;

struct Pair {
    lab: Lab,
    first: GatewayName,
    second: Arc<FakeGateway>,
    connector: Arc<ScriptedConnector>,
}

async fn pair(primed: bool) -> Out<Box<Pair>> {
    let mut lab = Lab::open().await?;
    let world = &lab.rig.world;
    let first = world.gateway.name().clone();
    let name = GatewayName::parse("second-gateway")?;
    let second = Arc::new(FakeGateway::new(
        name.clone(),
        OpenShellWorkspace::parse("default")?,
    ));
    second.link_engine(Arc::clone(&world.engine));
    second.link_clock(Arc::clone(&world.clock));
    second.use_host_home(world.home());
    let reply = |stdout: &[u8]| ProgramScript {
        exit_code: 0,
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
    };
    second.script_program(
        &sandbox_layout::PRINTENV,
        reply(format!("{PLACEHOLDER}\n").as_bytes()),
    );
    for kind in [HookKind::Setup, HookKind::Preflight] {
        let hook = sandbox_layout::hook_path(kind);
        second.install_file(&hook, b"#!/bin/sh\n".to_vec());
        second.script_program(&hook, reply(b""));
    }
    let entry = lab
        .config()
        .await?
        .gateway(&first)
        .cloned()
        .ok_or("the first gateway has no entry")?;
    lab.edit(vec![
        ConfigEdit::AddGateway {
            name: name.clone(),
            entry,
        },
        ConfigEdit::SetDefaultGateway(Some(first.clone())),
    ])
    .await?;
    let login = lab
        .config()
        .await?
        .account(&AccountName::parse("work")?)
        .cloned()
        .ok_or("the work account is missing")?;
    let peer: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&second);
    session_over(&lab.rig.world, Arc::clone(&peer))
        .sync_account(&login, &CancellationToken::new())
        .await?;
    let home: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&lab.rig.world.gateway);
    let connector = Arc::new(
        ScriptedConnector::new()
            .gateway(home, Vec::new())
            .gateway(Arc::clone(&peer), Vec::new()),
    );
    let mut builder = ConsoleRig::builder(&lab.rig.world)
        .connector(connector_over(
            &lab.rig.world,
            Arc::<ScriptedConnector>::clone(&connector),
        ))
        .size(200, 44);
    if primed {
        builder = builder.gateway(
            name,
            GatewayState::Connected(Box::new(session_over(&lab.rig.world, peer))),
        );
    }
    lab.console = builder.build().await?;
    lab.console.skip_intro();
    Ok(Box::new(Pair {
        lab,
        first,
        second,
        connector,
    }))
}

async fn refresh(lab: &mut Lab) -> TestResult {
    lab.console.elapse(5);
    lab.console.tick(5);
    lab.console.settle().await?;
    Ok(())
}

async fn health_row(lab: &mut Lab) -> Out<String> {
    lab.step("D").await?;
    lab.step("r").await?;
    let screen = lab.console.screen();
    let row = screen
        .lines()
        .filter(|line| line.contains(CheckId::GatewayHealth.label()))
        .find_map(|line| line.split('│').nth(1))
        .ok_or_else(|| format!("no gateway health row:\n{screen}"))?;
    Ok(row.to_owned())
}

fn provider_of(name: &str) -> Out<ProviderName> {
    Ok(ProviderName::for_account(
        &AccountName::parse(name)?,
        Agent::Codex,
    ))
}

#[tokio::test]
async fn doctor_checks_the_new_default_after_the_default_left_the_config() -> TestResult {
    guard(async {
        let mut pair = pair(true).await?;
        pair.second.set_serving(false);
        pair.lab
            .edit(vec![ConfigEdit::RemoveGateway(pair.first.clone())])
            .await?;
        refresh(&mut pair.lab).await?;
        let row = health_row(&mut pair.lab).await?;
        assert!(row.contains("fail"), "{row}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn doctor_keeps_the_default_while_it_stays_configured() -> TestResult {
    guard(async {
        let mut pair = pair(true).await?;
        pair.lab.rig.world.gateway.set_serving(false);
        refresh(&mut pair.lab).await?;
        let row = health_row(&mut pair.lab).await?;
        assert!(row.contains("fail"), "{row}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unchanged_config_reconnects_nothing() -> TestResult {
    guard(async {
        let mut pair = pair(true).await?;
        refresh(&mut pair.lab).await?;
        refresh(&mut pair.lab).await?;
        let second = pair.second.name().clone();
        assert_eq!(pair.connector.calls(&pair.first), 0);
        assert_eq!(pair.connector.calls(&second), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_account_add_publishes_to_the_new_default_after_the_default_left_the_config()
-> TestResult {
    guard(async {
        let mut pair = pair(false).await?;
        pair.lab
            .edit(vec![ConfigEdit::RemoveGateway(pair.first.clone())])
            .await?;
        refresh(&mut pair.lab).await?;
        let base = pair.lab.world().accounts.clone();
        let mut registry = base.clone();
        registry.accounts.push(Account::registered(
            UNSAVED_ID,
            "newlogin",
            Provider::OpenAi,
            CredentialSource::LocalFolder {
                path: pair.lab.rig.home.as_path().display().to_string(),
                detected: DetectedKind::CodexAuthJson,
            },
        ));
        pair.lab
            .perform(Effect::SaveAccounts {
                base: Box::new(base),
                registry: Box::new(registry),
            })
            .await?;
        assert!(pair.second.providers().contains(&provider_of("newlogin")?));
        assert!(
            !pair
                .lab
                .rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_runs_on_the_new_default_after_the_default_left_the_config() -> TestResult {
    guard(async {
        let mut pair = pair(true).await?;
        pair.lab
            .edit(vec![ConfigEdit::RemoveGateway(pair.first.clone())])
            .await?;
        refresh(&mut pair.lab).await?;
        pair.lab.step("down enter enter").await?;
        let mut asked = false;
        for _ in 0..4000 {
            pair.lab.console.tick(1);
            pair.lab.console.settle().await?;
            if pair.lab.console.console().app().route() == Route::Capsule {
                break;
            }
            if !asked && pair.lab.console.screen().contains("Values for this launch") {
                pair.lab.console.keys("tab tab enter")?;
                asked = true;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let screen = pair.lab.console.screen();
        assert_eq!(
            pair.lab.console.console().app().route(),
            Route::Capsule,
            "{screen}"
        );
        assert_eq!(pair.second.sandboxes().len(), 1);
        assert!(pair.lab.rig.world.gateway.sandboxes().is_empty());
        Ok(())
    })
    .await
}
