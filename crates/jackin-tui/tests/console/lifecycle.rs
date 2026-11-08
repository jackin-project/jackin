// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::Effect;
use jackin_console::domain::instance::{InstanceEntry, InstanceId, SandboxPhase};
use std::time::Duration;

use jackin_core::{Labels, Phase, WorkspaceName};
use jackin_openshell::Gateway;
use jackin_testkit::{TestResult, guard};

use super::support::Lab;

async fn launched() -> Result<(Lab, InstanceId), Box<dyn std::error::Error>> {
    let rig = jackin_testkit::Rig::new().await?;
    rig.launch(1).await?;
    let mut lab = Lab::with(rig).await?;
    let id = lab
        .world()
        .instances
        .first()
        .ok_or("no instance")?
        .id()
        .clone();
    Ok((lab, id))
}

#[tokio::test]
async fn stop_moves_a_ready_sandbox_to_stopped() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        lab.perform(Effect::Stop { instance: id }).await?;
        let InstanceEntry::Unavailable(instance) = &lab.world().instances[0] else {
            return Err("stopped instance has available metadata".into());
        };
        assert_eq!(instance.phase, SandboxPhase::Stopped);
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

const WORKSPACE: &str = "platform-backend";

async fn refused_with(lab: &mut Lab, name: &str) -> TestResult {
    let screen = lab.console.screen();
    let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(name) && flat.contains("export and purge"),
        "{screen}"
    );
    let config = lab.config().await?;
    assert!(
        config
            .workspace(&WorkspaceName::parse(WORKSPACE)?)
            .is_some()
    );
    assert!(
        config
            .workspace(&WorkspaceName::parse("renamed")?)
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn deleting_a_workspace_with_a_sandbox_is_refused_and_names_the_sandbox() -> TestResult {
    guard(async {
        let (mut lab, _) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        let id = lab.workspace(WORKSPACE).ok_or("no workspace")?.id;
        lab.perform(Effect::DeleteWorkspace { id }).await?;
        refused_with(&mut lab, name.as_str()).await
    })
    .await
}

#[tokio::test]
async fn renaming_a_workspace_with_a_sandbox_is_refused_and_names_the_sandbox() -> TestResult {
    guard(async {
        let (mut lab, _) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        lab.console.keys("down e enter")?;
        lab.console.type_text("renamed");
        lab.console.keys("enter")?;
        lab.save().await?;
        refused_with(&mut lab, name.as_str()).await
    })
    .await
}

#[tokio::test]
async fn stale_roster_power_refuses_same_name_replacement() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        let original = lab.rig.handle(1).await?;
        let name = original.name();
        let labels = original.labels();
        lab.rig.world.gateway.delete_sandbox(&name).await?;
        lab.rig.world.clock.advance(Duration::from_secs(60));
        lab.rig
            .world
            .gateway
            .insert_unmanaged(&name, labels, Phase::Ready)?;
        lab.perform(Effect::Stop { instance: id }).await?;
        let replacement = lab
            .rig
            .world
            .gateway
            .sandbox(&name)
            .ok_or("missing replacement")?;
        assert_eq!(replacement.phase(), Phase::Ready);
        let screen = lab.console.screen();
        assert!(
            screen.contains("replaced") || screen.contains("identity"),
            "{screen}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn stale_unavailable_roster_power_refuses_same_name_replacement() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        lab.perform(Effect::Stop {
            instance: id.clone(),
        })
        .await?;
        let original = lab.rig.handle(1).await?;
        let name = original.name();
        let labels = original.labels();
        lab.rig.world.gateway.delete_sandbox(&name).await?;
        lab.rig.world.clock.advance(Duration::from_secs(60));
        lab.rig
            .world
            .gateway
            .insert_unmanaged(&name, labels, Phase::Stopped)?;
        lab.perform(Effect::PowerStart { instance: id }).await?;
        let replacement = lab
            .rig
            .world
            .gateway
            .sandbox(&name)
            .ok_or("missing replacement")?;
        assert_eq!(replacement.phase(), Phase::Stopped);
        let screen = lab.console.screen();
        assert!(
            screen.contains("replaced") || screen.contains("identity"),
            "{screen}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn power_stops_metadata_unavailable_row_with_complete_lifecycle_identity() -> TestResult {
    guard(async {
        let rig = jackin_testkit::Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let labels = Labels::from_pairs(
            handle
                .labels()
                .iter()
                .filter(|(key, _)| key.to_string() != "jackin.dev/agents")
                .map(|(key, value)| (key.to_string(), value.to_string())),
        )?;
        handle.set_labels(labels);
        let mut lab = Lab::with(rig).await?;
        let InstanceEntry::Unavailable(entry) = &lab.world().instances[0] else {
            return Err("expected unavailable agent metadata".into());
        };
        let id = entry.id.clone();
        lab.perform(Effect::PowerStop { instance: id }).await?;
        assert_eq!(handle.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}
