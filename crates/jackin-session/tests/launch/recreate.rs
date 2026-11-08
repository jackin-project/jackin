// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Agent, FileMode, sandbox_layout};
use jackin_image::{Rebuild, ensure_image};
use jackin_openshell::Gateway;
use jackin_testkit::SupervisorModel;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, actual_final_builds, converged, guard, has_file, launch_rig, path, ready, running,
    script_construct_rebuild, write,
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
        let (plan, builds, _) = actual_final_builds(&rig, 1).await?;
        let peer = async {
            while !rig
                .world
                .clock
                .slept()
                .contains(&Duration::from_millis(500))
            {
                tokio::task::yield_now().await;
            }
            let original = rig
                .world
                .docker
                .image_id(&builds.derived.tag)
                .ok_or("the initial final image is missing")?;
            let old = rig
                .gateway()
                .sandbox(&name)
                .ok_or("the original sandbox is missing")?;
            let original_snapshot = old
                .image_id()
                .ok_or("the original sandbox has no image snapshot")?;
            let original_ref = jackin_core::LocalImageRef::for_image(&original_snapshot);
            let original_label = old
                .labels()
                .iter()
                .find(|(name, _)| name.to_string() == "jackin.dev/image")
                .map(|(_, value)| value.to_string())
                .ok_or("the original sandbox has no image label")?;
            assert_eq!(original_snapshot, original);
            assert_eq!(original_label, original_snapshot.label_value().to_string());
            rig.repo
                .file("while-launch-waits.txt", b"external rebuild\n")
                .await?;
            let (construct_tag, construct_id) = script_construct_rebuild(&rig, 1, 'e')?;
            let mut request = rig.plan(1)?.image_request().clone();
            request.rebuild = Rebuild::Yes;
            let (progress, _feed) = mpsc::channel(256);
            let rebuilt = ensure_image(
                &*rig.world.engine,
                &request,
                &progress,
                &CancellationToken::new(),
            )
            .await?;
            assert_ne!(rebuilt.id, original);
            assert_eq!(
                rig.world.docker.local_image_id(&original_ref),
                Some(original_snapshot.clone())
            );
            assert_eq!(
                rig.world.docker.local_image_id(&rebuilt.local_ref),
                Some(rebuilt.id.clone())
            );
            assert_eq!(rebuilt.tag, plan.final_tag(&construct_id));
            assert_eq!(
                rig.world.docker.image_id(&construct_tag),
                Some(construct_id)
            );
            assert_eq!(
                rig.world.docker.image_id(&rebuilt.tag),
                Some(rebuilt.id.clone())
            );
            assert_eq!(
                rig.world.docker.image_id(&builds.derived.tag),
                Some(original.clone())
            );
            rig.world
                .docker
                .retag(builds.derived.tag.clone(), Some(rebuilt.id.clone()));
            assert_eq!(
                rig.world.docker.image_id(&builds.derived.tag),
                Some(rebuilt.id.clone())
            );
            rig.gateway().set_supervisor(SupervisorModel::Cooperative);
            rig.gateway().delete_sandbox(&name).await?;
            Ok::<_, Box<dyn std::error::Error>>((original_snapshot, original_label, rebuilt.id))
        };
        let (outcome, result) = tokio::join!(rig.launch_observed(1), peer);
        let (original_snapshot, original_label, rebuilt) = result?;
        ready(outcome?)?;

        assert_eq!(
            rig.world.docker.image_id(&builds.derived.tag),
            Some(rebuilt.clone())
        );
        let handle = rig.handle(1).await?;
        let ran = handle.image_id().ok_or("the sandbox has no image")?;
        assert_eq!(ran, original_snapshot);
        assert_eq!(
            rig.world
                .docker
                .local_image_id(&jackin_core::LocalImageRef::for_image(&original_snapshot)),
            Some(original_snapshot.clone())
        );
        let labelled = handle
            .labels()
            .iter()
            .find(|(name, _)| name.to_string() == "jackin.dev/image")
            .map(|(_, value)| value.to_string());
        assert_eq!(
            Some(original_label),
            labelled,
            "the recreated sandbox changed its captured image label"
        );
        assert_eq!(handle.supervisor(), Some(running(1)?));
        Ok(())
    })
    .await
}
