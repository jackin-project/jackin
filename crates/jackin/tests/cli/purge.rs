// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{Labels, Phase, SandboxName};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_testkit::{Fault, Rig, TestResult, guard};

use super::gateway_choice::with_away_gateway;
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
async fn purging_an_absent_sandbox_is_ok_and_an_unmanaged_one_is_refused() -> TestResult {
    guard(async {
        let (rig, harness, clean, _dirty) = two().await?;
        harness.run(&["purge", "jackin-absent-0009"]).await?.ok()?;
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        let refused = harness.run(&["purge", ghost.as_str(), "--force"]).await?;
        failed(&refused, ExitClass::Refused, &["not a jackin sandbox"])?;
        assert!(!refused.stderr.contains("already gone"));
        assert!(rig.world.gateway.sandbox(&ghost).is_some());
        assert!(rig.world.gateway.sandbox(&clean).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unconfirmed_purge_names_the_gateway_that_received_the_request() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let initial = rig.handle(1).await?;
        let name = initial.name();
        let (harness, away) = with_away_gateway(&rig, &initial)?;
        away.inject(Fault::FailNext {
            op: GatewayOp::Delete,
            error: GatewayError::Timeout {
                op: GatewayOp::Delete,
                after: Duration::from_secs(1),
                group: None,
            },
        });

        let ran = harness
            .run(&["purge", name.as_str(), "--force", "--gateway", "away"])
            .await?;
        failed(
            &ran,
            ExitClass::Failure,
            &[
                name.as_str(),
                "could not confirm",
                "jackin ls --gateway away",
            ],
        )?;
        let kept = away.sandbox(&name).ok_or("the selected sandbox is gone")?;
        assert_eq!(kept.phase(), Phase::Ready);
        assert_eq!(initial.phase(), Phase::Ready);
        Ok(())
    })
    .await
}
