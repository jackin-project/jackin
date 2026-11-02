// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{Labels, Phase, SandboxName};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{Failure, Harness, failed};

async fn two() -> Result<(Rig, Harness, SandboxName, SandboxName), Failure> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    rig.launch(2).await?;
    let (clean, dirty) = (rig.sandbox_name(1)?, rig.sandbox_name(2)?);
    rig.handle(2)
        .await?
        .commit_in_project("work.txt", b"w\n", "work")
        .await?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness, clean, dirty))
}

#[tokio::test]
async fn purge_deletes_an_exported_sandbox_refuses_unexported_work_and_force_deletes_it()
-> TestResult {
    guard(async {
        let (rig, harness, clean, dirty) = two().await?;
        harness.run(&["purge", clean.as_str()]).await?.ok()?;
        assert!(rig.world.gateway.sandbox(&clean).is_none());
        assert!(rig.world.gateway.sandbox(&dirty).is_some());

        let refused = harness.run(&["purge", dirty.as_str()]).await?;
        failed(&refused, ExitClass::Refused, &[dirty.as_str()])?;
        assert!(rig.world.gateway.sandbox(&dirty).is_some());

        harness
            .run(&["purge", dirty.as_str(), "--force"])
            .await?
            .ok()?;
        assert!(rig.world.gateway.sandbox(&dirty).is_none());
        assert!(!rig.world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn purging_an_absent_sandbox_is_ok_and_an_unmanaged_one_is_never_deleted() -> TestResult {
    guard(async {
        let (rig, harness, clean, _dirty) = two().await?;
        harness.run(&["purge", "jackin-absent-0009"]).await?.ok()?;
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        harness
            .run(&["purge", ghost.as_str(), "--force"])
            .await?
            .ok()?;
        assert!(rig.world.gateway.sandbox(&ghost).is_some());
        assert!(rig.world.gateway.sandbox(&clean).is_some());
        Ok(())
    })
    .await
}
