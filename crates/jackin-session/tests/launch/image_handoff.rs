// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicBool, Ordering};

use jackin_core::ImageId;
use jackin_image::{BuildStage, Rebuild, ensure_image};
use jackin_openshell::GatewayOp;
use jackin_testkit::{BuildEffect, EngineMoment, Fault, Trigger};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, launch_rig, ready};

#[tokio::test]
async fn a_rebuild_after_the_image_step_never_changes_the_image_the_sandbox_runs() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let create = Trigger::Op(GatewayOp::Create);
        rig.gateway().inject(Fault::HoldAt {
            trigger: create.clone(),
        });
        let plan = rig.plan(1)?;
        let mut request = plan.image_request().clone();
        request.rebuild = Rebuild::Yes;
        let other = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
        let finished = AtomicBool::new(false);
        let (start_rival, start) = oneshot::channel::<()>();
        let launching = rig.run_planned(&rig.world.session, plan, CancellationToken::new());
        let rival = async {
            start.await?;
            let (tx, _rx) = mpsc::channel(256);
            let built =
                ensure_image(&*rig.world.engine, &request, &tx, &CancellationToken::new()).await;
            finished.store(true, Ordering::SeqCst);
            Ok::<_, Box<dyn std::error::Error>>(built)
        };
        let driver = async {
            assert!(
                rig.gateway().until_held(&create).await,
                "the launch never reached the create"
            );
            let final_tag = rig.final_image()?.tag;
            rig.world.engine.at(EngineMoment::BuildFinished {
                stage: BuildStage::Derived,
                effect: BuildEffect::NewId(other.clone()),
            });
            start_rival.send(()).map_err(|()| "the rival is gone")?;
            while rig.world.engine.waiters(&final_tag) == 0 && !finished.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            rig.gateway().release(&create);
            Ok::<_, Box<dyn std::error::Error>>(final_tag)
        };
        let (outcome, rival, final_tag) = tokio::join!(launching, rival, driver);
        let final_tag = final_tag?;
        ready(outcome)?;
        rival??;

        let handle = rig.handle(1).await?;
        let ran = handle.image_id().ok_or("the sandbox has no image")?;
        let labelled = handle
            .labels()
            .iter()
            .find(|(name, _)| name.to_string() == "jackin.dev/image")
            .map(|(_, value)| value.to_string());
        assert_eq!(
            Some(ran.label_value().to_string()),
            labelled,
            "the sandbox runs another image than its label records"
        );
        assert_eq!(
            rig.world.engine.images().get(&final_tag),
            Some(&other),
            "the rival never rebuilt the image"
        );
        Ok(())
    })
    .await
}
