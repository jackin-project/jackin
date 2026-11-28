// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{Labels, Phase, SandboxName, SandboxPath, SupervisorState};
use jackin_openshell::GatewayOp;
use jackin_testkit::{FakeGateway, Fault, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::gateway_choice::with_away_gateway;
use super::support::{Failure, Harness, Ran, failed};

fn project_file(path: &str) -> Result<SandboxPath, Failure> {
    Ok(SandboxPath::parse(path)?)
}

#[tokio::test]
async fn stop_then_start_keeps_the_files_and_returns_the_supervisor_to_waiting() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let harness = Harness::new(&rig.world);
        harness.write_config(&rig.config.render())?;
        let source = project_file("/sandbox/project/src/main.rs")?;
        let kept = handle.file(&source);
        assert!(kept.is_some());

        harness.run(&["stop", name.as_str()]).await?.ok()?;
        assert_eq!(handle.phase(), Phase::Stopped);
        harness.run(&["start", name.as_str()]).await?.ok()?;
        assert_eq!(handle.phase(), Phase::Ready);
        assert_eq!(handle.file(&source), kept);
        assert_eq!(handle.supervisor(), Some(SupervisorState::Waiting));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_errored_unknown_or_unmanaged_sandbox_cannot_be_stopped_or_started() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        rig.world
            .gateway
            .force_phase(&name, Phase::Error { exit_code: Some(3) });
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        let harness = Harness::new(&rig.world);
        harness.write_config(&rig.config.render())?;

        for verb in ["stop", "start"] {
            let ran = harness.run(&[verb, name.as_str()]).await?;
            failed(&ran, ExitClass::Failure, &[name.as_str(), "3"])?;
            assert_eq!(handle.phase(), Phase::Error { exit_code: Some(3) });
            failed(
                &harness.run(&[verb, "jackin-absent-0009"]).await?,
                ExitClass::Failure,
                &["jackin-absent-0009"],
            )?;
            failed(
                &harness.run(&[verb, ghost.as_str()]).await?,
                ExitClass::Refused,
                &[ghost.as_str(), "not a jackin sandbox"],
            )?;
        }
        let ghost_handle = rig.world.gateway.sandbox(&ghost).ok_or("ghost gone")?;
        assert_eq!(ghost_handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

async fn interrupted_stop(
    gateway: &FakeGateway,
    name: &SandboxName,
    harness: &Harness,
    held_op: GatewayOp,
    interrupt_twice: bool,
) -> Result<Ran, Failure> {
    let handle = gateway
        .sandbox(name)
        .ok_or("no sandbox on the selected gateway")?;
    let wait = Duration::from_secs(10);
    let hold = Trigger::Op(held_op);
    gateway.inject(Fault::HoldAt {
        trigger: hold.clone(),
    });
    let cancel = CancellationToken::new();
    let abandon = CancellationToken::new();
    let stop = harness
        .command(&["stop", name.as_str(), "--gateway", "away"])
        .cancel(cancel.clone())
        .abandon(abandon.clone())
        .run();
    tokio::pin!(stop);
    let driver = async {
        let reached = tokio::time::timeout(wait, gateway.until_held(&hold)).await;
        cancel.cancel();
        if interrupt_twice {
            abandon.cancel();
        }
        reached
    };
    let (reached, result) = tokio::join!(driver, tokio::time::timeout(wait + wait, &mut stop));
    let phase = handle.phase();
    assert!(gateway.release(&hold));
    let Ok(ran) = result else {
        drop(tokio::time::timeout(wait, &mut stop).await);
        return Err("stop still waited for the gateway after interruption".into());
    };

    assert_eq!(reached, Ok(true));
    assert_eq!(phase, Phase::Ready);
    assert_eq!(handle.phase(), Phase::Ready);
    ran
}

#[tokio::test]
async fn a_second_interrupt_reports_the_unsettled_stop() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let initial = rig.handle(1).await?;
        let name = initial.name();
        let (harness, away) = with_away_gateway(&rig, &initial)?;
        let cancelled = interrupted_stop(&away, &name, &harness, GatewayOp::Find, false).await?;
        failed(&cancelled, ExitClass::Cancelled, &["cancelled"])?;
        assert!(!cancelled.stderr.contains("interrupted again"));
        let abandoned = interrupted_stop(&away, &name, &harness, GatewayOp::Power, true).await?;
        failed(
            &abandoned,
            ExitClass::Cancelled,
            &[
                "interrupted again",
                name.as_str(),
                "stop request",
                "jackin ls --gateway away",
            ],
        )?;
        Ok(())
    })
    .await
}
