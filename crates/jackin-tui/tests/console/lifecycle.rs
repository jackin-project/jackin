// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::Effect;
use jackin_console::domain::instance::InstanceStatus;
use jackin_core::Phase;
use jackin_testkit::{TestResult, guard};

use super::support::Lab;

async fn launched() -> Result<(Lab, String), Box<dyn std::error::Error>> {
    let rig = jackin_testkit::Rig::new().await?;
    rig.launch(1).await?;
    let mut lab = Lab::with(rig).await?;
    let id = lab
        .world()
        .instances
        .first()
        .ok_or("no instance")?
        .id
        .clone();
    Ok((lab, id))
}

#[tokio::test]
async fn stop_moves_a_ready_sandbox_to_stopped() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        lab.perform(Effect::Stop { instance: id }).await?;
        assert_eq!(
            lab.world().instances[0].status,
            InstanceStatus::RestoreAvailable
        );
        assert_eq!(lab.rig.handle(1).await?.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn stop_on_a_completed_sandbox_says_the_main_process_ended() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        lab.rig
            .world
            .gateway
            .force_phase(&name, Phase::Completed { exit_code: 0 });
        lab.perform(Effect::Stop { instance: id }).await?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("main process of") && screen.contains("has ended"),
            "{screen}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_guarded_purge_refuses_unexported_work() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        let handle = lab.rig.handle(1).await?;
        handle.commit_in_project("c.txt", b"c", "work").await?;
        lab.perform(Effect::Purge { instance: id }).await?;
        let screen = lab.console.screen();
        assert!(screen.contains("is not exported"), "{screen}");
        assert!(lab.rig.world.gateway.sandbox(&handle.name()).is_some());
        assert_eq!(lab.world().instances.len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn purge_deletes_a_sandbox_with_nothing_to_export() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        lab.perform(Effect::Purge { instance: id }).await?;
        assert!(lab.rig.world.gateway.sandbox(&name).is_none());
        assert!(lab.world().instances.is_empty());
        Ok(())
    })
    .await
}
