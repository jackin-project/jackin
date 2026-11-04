// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_console::domain::instance::InstanceStatus;
use jackin_core::{GatewayName, OpenShellWorkspace};
use jackin_openshell::{Gateway, GatewayOp, PowerAction};
use jackin_testkit::{
    ConsoleRig, FakeGateway, Fault, Rig, TestResult, Trigger, guard, session_over,
};
use jackin_tui::GatewayState;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(1);

async fn refresh(console: &mut ConsoleRig) -> TestResult {
    console.elapse(5);
    console.tick(5);
    console.settle().await?;
    Ok(())
}

fn statuses(console: &mut ConsoleRig) -> Vec<InstanceStatus> {
    let mut found: Vec<InstanceStatus> = console
        .console()
        .app()
        .world
        .instances
        .iter()
        .map(|instance| instance.status)
        .collect();
    found.sort_by_key(|status| format!("{status:?}"));
    found
}

fn second_gateway(
    name: &str,
) -> Result<(GatewayName, Arc<FakeGateway>), Box<dyn std::error::Error>> {
    let name = GatewayName::parse(name)?;
    let gateway = Arc::new(FakeGateway::new(
        name.clone(),
        OpenShellWorkspace::parse("default")?,
    ));
    Ok((name, gateway))
}

#[tokio::test]
async fn slow_gateway_keeps_the_fast_listing() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.seed_all(&rig.config).await?;
        let (name, slow) = second_gateway("slow-gateway")?;
        slow.inject(Fault::HoldAt {
            trigger: Trigger::Op(GatewayOp::Find),
        });
        let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&slow);
        let session = session_over(&rig.world, port);
        let mut console = ConsoleRig::builder(&rig.world)
            .gateway(name, GatewayState::Connected(Box::new(session)))
            .deadline(DEADLINE)
            .build()
            .await?;
        assert_eq!(statuses(&mut console), [InstanceStatus::Running]);
        let status = console.status_line();
        assert!(status.contains("slow-gateway"), "{status}");
        assert!(status.contains("no answer within 1s"), "{status}");
        assert!(!status.contains("fake-gateway"), "{status}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn slow_inspect_keeps_the_sandbox_listed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: Trigger::Op(GatewayOp::Detail),
        });
        let mut console = ConsoleRig::builder(&rig.world)
            .deadline(DEADLINE)
            .build()
            .await?;
        assert_eq!(
            statuses(&mut console),
            [InstanceStatus::RestoreAvailable, InstanceStatus::Running]
        );
        assert!(
            console.status_line().is_empty(),
            "{}",
            console.status_line()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn slow_work_read_keeps_the_sandbox_listed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.handle(1).await?.exit_codex(0);
        rig.launch(2).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: Trigger::Op(GatewayOp::Program),
        });
        let mut console = ConsoleRig::builder(&rig.world)
            .deadline(DEADLINE)
            .build()
            .await?;
        assert_eq!(
            statuses(&mut console),
            [InstanceStatus::CleanExited, InstanceStatus::Running]
        );
        assert!(
            console.status_line().is_empty(),
            "{}",
            console.status_line()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn work_state_is_read_once_per_state() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        rig.handle(1).await?.exit_codex(0);
        let gateway = Arc::clone(&rig.world.gateway);
        let program = || gateway.calls(GatewayOp::Program);
        let before = program();
        let mut console = ConsoleRig::seeded(&rig).await?;
        console.skip_intro();
        let one_read = program() - before;
        assert!(one_read > 0, "the exited sandbox is not read at start");

        refresh(&mut console).await?;
        refresh(&mut console).await?;
        assert_eq!(
            program() - before,
            one_read,
            "an unchanged sandbox is read again"
        );

        rig.handle(2).await?.exit_codex(0);
        refresh(&mut console).await?;
        assert_eq!(
            program() - before,
            2 * one_read,
            "a changed sandbox is not read"
        );

        refresh(&mut console).await?;
        assert_eq!(
            program() - before,
            2 * one_read,
            "an unchanged sandbox is read again"
        );
        assert_eq!(
            statuses(&mut console),
            [InstanceStatus::CleanExited, InstanceStatus::CleanExited]
        );

        let first = rig.sandbox_name(1)?;
        for action in [PowerAction::Stop, PowerAction::Start] {
            rig.world
                .session
                .power(&first, action, &CancellationToken::new())
                .await?;
        }
        refresh(&mut console).await?;
        assert_eq!(
            program() - before,
            3 * one_read,
            "a restarted sandbox is not read"
        );
        Ok(())
    })
    .await
}
