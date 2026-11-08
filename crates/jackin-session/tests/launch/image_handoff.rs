// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    DockerLabel, FileSystem, FsError, HostPath, ImageId, LocalImageRef, SandboxPath, Sha256,
    StdFileSystem,
};
use jackin_image::{ImageEngine, ImageSource, Rebuild, ensure_image};
use jackin_openshell::{
    ClosedPolicy, Gateway, GatewayError, GatewayOp, MainProcess, SandboxCreate,
};
use jackin_testkit::{DockerBuildReply, DockerIidFile, Fault, LockSignalFs, Trigger};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, actual_final_builds, builder, guard, ready};

#[tokio::test]
async fn a_rebuild_after_the_image_step_never_changes_the_image_the_sandbox_runs() -> TestResult {
    guard(async {
        let (contended_tx, mut contended_rx) = mpsc::unbounded_channel();
        let fs = Arc::new(LockSignalFs::new(Arc::new(StdFileSystem::default()), contended_tx));
        let rig = builder().await?.filesystem(fs).build().await?;
        let create = Trigger::Op(GatewayOp::Create);
        rig.gateway().inject(Fault::HoldAt {
            trigger: create.clone(),
        });
        let plan = rig.plan(1)?;
        let mut request = plan.image_request().clone();
        request.rebuild = Rebuild::Yes;
        let mut launching = Box::pin(
            rig.run_planned(&rig.world.session, plan, CancellationToken::new()),
        );
        tokio::select! {
            result = &mut launching => return Err(format!("the launch finished before the create was held: {result:?}").into()),
            held = rig.gateway().until_held(&create) => assert!(held, "the launch never reached the create"),
        }
        let (_, builds, _) = actual_final_builds(&rig, 1).await?;
        let original = rig
            .world
            .docker
            .image_id(&builds.derived.tag)
            .ok_or("the launch has no final image in the external Docker state")?;
        let lock_hash = Sha256::of(builds.derived.tag.to_string().as_bytes());
        let expected_lock = HostPath::parse(
            &rig.world
                .image_work_dir()
                .as_path()
                .join("locks")
                .join(format!("{}.lock", lock_hash.as_str())),
        )?;
        match StdFileSystem::default().lock(&expected_lock).await {
            Err(FsError::LockHeld(_)) => {}
            Ok(lock) => {
                drop(lock);
                return Err("the launch released its final image lock before Create".into());
            }
            Err(error) => return Err(error.into()),
        }
        let replacement = ImageId::parse(&format!(
            "sha256:{}",
            Sha256::of(b"replacement published after launch create").as_str()
        ))?;
        rig.world.docker.script_build(
            builds.derived.tag.clone(),
            DockerBuildReply::default()
                .with_iid_file(DockerIidFile::Text(format!("{replacement}\n").into_bytes()))
                .with_published_id(replacement.clone()),
        );
        let engine = Arc::clone(&rig.world.engine);
        let rival_request = request.clone();
        let mut rival = tokio::spawn(async move {
            let (tx, _rx) = mpsc::channel(256);
            ensure_image(&*engine, &rival_request, &tx, &CancellationToken::new()).await
        });
        for _ in 0..2 {
            tokio::select! {
                result = &mut rival => return Err(format!("the rival finished without waiting on the held image lock: {result:?}").into()),
                path = contended_rx.recv() => {
                    assert_eq!(path.ok_or("the filesystem contention signal closed")?, expected_lock);
                }
            }
            assert!(
                !rival.is_finished(),
                "the rival completed while the launch still held the image lock"
            );
        }
        assert_eq!(rig.world.docker.image_id(&builds.derived.tag), Some(original.clone()));
        rig.gateway().release(&create);
        ready(launching.await)?;
        let rebuilt = rival.await??;
        assert_eq!(rebuilt.source, ImageSource::Built);
        assert_eq!(rebuilt.tag, builds.derived.tag);
        assert_eq!(rebuilt.id, replacement);
        assert_ne!(rebuilt.id, original);
        let (_, _, rebuilt_construct) = actual_final_builds(&rig, 1).await?;

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
        assert_eq!(ran, original);
        let original_ref = LocalImageRef::for_image(&original);
        assert_eq!(
            rig.world.docker.local_image_id(&original_ref),
            Some(original.clone())
        );
        assert_eq!(
            rig.world.docker.local_image_id(&rebuilt.local_ref),
            Some(rebuilt.id.clone())
        );
        assert_eq!(
            rig.world.docker.image_id(&builds.derived.tag),
            Some(rebuilt.id.clone())
        );
        let construct_inspect = rig
            .world
            .engine
            .inspect(&rebuilt_construct, &CancellationToken::new())
            .await?;
        let rebuilt_inspect = rig
            .world
            .engine
            .inspect(&rebuilt.id, &CancellationToken::new())
            .await?;
        assert!(rebuilt_inspect.layers.starts_with(&construct_inspect.layers));
        Ok(())
    })
    .await
}

fn create_request(
    rig: &jackin_testkit::Rig,
    plan: &jackin_session::LaunchPlan,
    image: &jackin_image::BuiltImage,
) -> Result<SandboxCreate, Box<dyn std::error::Error>> {
    Ok(SandboxCreate::new(
        plan.sandbox_name().clone(),
        image.local_ref.clone(),
        plan.draft()
            .labels()
            .clone()
            .complete(&image.id, DockerLabel::Off),
        Vec::new(),
        ClosedPolicy::parse(&rig.loaded.policy().document().render()?)?,
        MainProcess {
            program: SandboxPath::parse("/bin/sh")?,
            args: vec!["-c".to_owned(), "sleep 3600".to_owned()],
        },
    ))
}

#[tokio::test]
async fn a_missing_native_pin_fails_closed_even_when_the_mutable_build_tag_exists() -> TestResult {
    guard(async {
        let rig = builder().await?.build().await?;
        let plan = rig.plan(1)?;
        let (progress, _events) = mpsc::channel(256);
        let image = ensure_image(
            &*rig.world.engine,
            plan.image_request(),
            &progress,
            &CancellationToken::new(),
        )
        .await?;
        let create = create_request(&rig, &plan, &image)?;
        rig.world.docker.retag_local(image.local_ref.clone(), None);
        assert_eq!(
            rig.world.docker.image_id(&image.tag),
            Some(image.id.clone())
        );

        let result = rig.gateway().create_sandbox(&create).await;
        assert!(matches!(
            result,
            Err(GatewayError::FailedPrecondition { .. })
        ));
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancellation_during_pin_publication_keeps_shared_state_safe_and_retryable() -> TestResult {
    guard(async {
        let rig = builder().await?.build().await?;
        let plan = rig.plan(1)?;
        let request = plan.image_request().clone();
        let (progress, _events) = mpsc::channel(256);
        let first = ensure_image(
            &*rig.world.engine,
            &request,
            &progress,
            &CancellationToken::new(),
        )
        .await?;
        rig.world.docker.retag_local(first.local_ref.clone(), None);
        let gate = rig.world.docker.hold_local_tag(first.local_ref.clone());
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let engine = Arc::clone(&rig.world.engine);
        let (task_progress, _task_events) = mpsc::channel(256);
        let task_request = request.clone();
        let task = tokio::spawn(async move {
            ensure_image(engine.as_ref(), &task_request, &task_progress, &task_cancel).await
        });
        gate.wait_arrived().await;
        cancel.cancel();
        let canceled = task.await?;
        assert!(matches!(
            canceled,
            Err(jackin_image::ImageError::Cancelled { .. })
        ));
        assert_eq!(
            rig.world.docker.local_image_id(&first.local_ref),
            None,
            "the canceled external metadata request did not publish the pin"
        );
        assert!(rig.gateway().sandboxes().is_empty());

        let retry = ensure_image(
            &*rig.world.engine,
            &request,
            &progress,
            &CancellationToken::new(),
        )
        .await?;
        assert_eq!(retry.id, first.id);
        assert_eq!(retry.local_ref, first.local_ref);
        let shared_cancel = CancellationToken::new();
        shared_cancel.cancel();
        assert!(matches!(
            rig.world
                .engine
                .publish_local(&retry.id, &shared_cancel)
                .await,
            Err(jackin_image::EngineError::Cancelled { .. })
        ));
        assert_eq!(
            rig.world.docker.local_image_id(&retry.local_ref),
            Some(retry.id.clone())
        );
        let create = create_request(&rig, &plan, &retry)?;
        rig.gateway().create_sandbox(&create).await?;
        let handle = rig
            .gateway()
            .sandbox(plan.sandbox_name())
            .ok_or("the created sandbox is missing")?;
        assert_eq!(handle.image_id(), Some(first.id.clone()));
        assert_eq!(
            rig.world.docker.local_image_id(&first.local_ref),
            Some(first.id.clone())
        );
        let image_label = handle
            .labels()
            .iter()
            .find(|(name, _)| name.to_string() == "jackin.dev/image")
            .map(|(_, value)| value.to_string());
        assert_eq!(image_label, Some(first.id.label_value().to_string()));
        Ok(())
    })
    .await
}
