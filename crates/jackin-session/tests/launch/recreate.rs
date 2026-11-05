// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Agent, FileMode, ImageId, sandbox_layout};
use jackin_image::{BuildStage, Rebuild, ensure_image};
use jackin_openshell::Gateway;
use jackin_testkit::{BuildEffect, EngineMoment, SupervisorModel};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, converged, guard, has_file, launch_rig, path, ready, running, write,
};

#[tokio::test]
async fn a_sandbox_deleted_after_it_was_ready_is_recreated_and_gets_its_credentials_again()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway()
            .set_supervisor(SupervisorModel::NeverWritesState);
        let name = rig.sandbox_name(1)?;
        let peer = async {
            while !rig
                .world
                .clock
                .slept()
                .contains(&Duration::from_millis(500))
            {
                tokio::task::yield_now().await;
            }
            rig.gateway().set_supervisor(SupervisorModel::Cooperative);
            rig.gateway().delete_sandbox(&name).await
        };
        let (outcome, deleted) = tokio::join!(rig.launch_observed(1), peer);
        deleted?;
        ready(outcome?)?;

        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        let spec = Agent::Codex.spec();
        assert_eq!(
            handle.file_mode(&spec.auth_file.path),
            Some(FileMode(0o600))
        );
        assert!(has_file(
            &handle,
            &sandbox_layout::env_file(&super::support::launch_id(1))
        ));
        assert!(has_file(&handle, &path("/sandbox/project/src/main.rs")?));
        converged(&rig, &handle, 1)
    })
    .await
}

#[tokio::test]
async fn a_sandbox_deleted_after_the_start_request_is_recreated_and_gets_its_own_start()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway()
            .set_supervisor(SupervisorModel::NeverWritesState);
        let name = rig.sandbox_name(1)?;
        let peer = async {
            while !rig
                .world
                .clock
                .slept()
                .contains(&Duration::from_millis(500))
            {
                tokio::task::yield_now().await;
            }
            let old = rig
                .gateway()
                .sandbox(&name)
                .ok_or("the sandbox is missing")?;
            write(&rig, &name, &sandbox_layout::RUN_STATE, "waiting\n").await?;
            while !has_file(&old, &sandbox_layout::RUN_START) {
                tokio::task::yield_now().await;
            }
            let seen = rig.world.clock.slept().len();
            while rig.world.clock.slept().len() == seen {
                tokio::task::yield_now().await;
            }
            rig.gateway().set_supervisor(SupervisorModel::Cooperative);
            rig.gateway().delete_sandbox(&name).await?;
            Ok::<_, Box<dyn std::error::Error>>(old)
        };
        let (outcome, old) = tokio::join!(rig.launch_observed(1), peer);
        let old = old?;
        let target = ready(outcome?)?;

        assert_eq!(target.state, running(1)?);
        assert_eq!(old.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_ne!(old.supervisor(), Some(running(1)?));
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        converged(&rig, &handle, 1)
    })
    .await
}

#[tokio::test]
async fn a_sandbox_deleted_after_a_rebuild_moved_the_tag_is_recreated_from_the_image_it_is_labelled_with()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway()
            .set_supervisor(SupervisorModel::NeverWritesState);
        let name = rig.sandbox_name(1)?;
        let mut request = rig.plan(1)?.image_request().clone();
        request.rebuild = Rebuild::Yes;
        let other = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
        let peer = async {
            while !rig
                .world
                .clock
                .slept()
                .contains(&Duration::from_millis(500))
            {
                tokio::task::yield_now().await;
            }
            rig.world.engine.at(EngineMoment::BuildFinished {
                stage: BuildStage::Derived,
                effect: BuildEffect::NewId(other.clone()),
            });
            let (progress, _feed) = mpsc::channel(256);
            ensure_image(
                &*rig.world.engine,
                &request,
                &progress,
                &CancellationToken::new(),
            )
            .await?;
            rig.gateway().set_supervisor(SupervisorModel::Cooperative);
            rig.gateway().delete_sandbox(&name).await?;
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (outcome, rebuilt) = tokio::join!(rig.launch_observed(1), peer);
        rebuilt?;
        ready(outcome?)?;

        let final_tag = rig.final_image()?.tag;
        assert_eq!(rig.world.engine.images().get(&final_tag), Some(&other));
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
        assert_eq!(handle.supervisor(), Some(running(1)?));
        Ok(())
    })
    .await
}
